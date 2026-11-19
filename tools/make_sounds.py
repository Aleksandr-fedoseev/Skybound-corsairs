#!/usr/bin/env python3
"""Синтезирует звуки и музыку игры: crates/game/assets/audio/*.ogg.

Всё процедурное (numpy) — без чужих сэмплов и лицензий. WAV кодируется в OGG Vorbis
через ffmpeg. Запуск из корня репозитория: python3 tools/make_sounds.py
"""

import subprocess
import tempfile
import wave
from pathlib import Path

import numpy as np

SR = 44100
OUT = Path("crates/game/assets/audio")
rng = np.random.default_rng(7)


# ---------- основа ----------

def t_of(seconds):
    return np.arange(int(seconds * SR)) / SR


def silence(seconds):
    return np.zeros(int(seconds * SR))


def noise(seconds):
    return rng.uniform(-1, 1, int(seconds * SR))


def env_exp(seconds, decay, attack=0.002):
    t = t_of(seconds)
    a = np.clip(t / max(attack, 1e-4), 0, 1)
    return a * np.exp(-t / decay)


def env_adsr(seconds, attack, release, hold_level=1.0):
    t = t_of(seconds)
    a = np.clip(t / max(attack, 1e-4), 0, 1)
    r = np.clip((seconds - t) / max(release, 1e-4), 0, 1)
    return a * r * hold_level


def filt(x, lo=None, hi=None, order=2):
    """Полосовой фильтр в частотной области (мягкие склоны, как у Баттерворта)."""
    n = len(x)
    size = 1 << (n - 1).bit_length()
    spec = np.fft.rfft(x, size)
    f = np.fft.rfftfreq(size, 1 / SR)
    gain = np.ones_like(f)
    if hi:
        gain *= 1 / np.sqrt(1 + (f / hi) ** (2 * order))
    if lo:
        gain *= 1 / np.sqrt(1 + (lo / np.maximum(f, 1e-3)) ** (2 * order))
    return np.fft.irfft(spec * gain, size)[:n]


def sweep(f0, f1, seconds, shape=np.sin, curve=1.0):
    t = t_of(seconds)
    k = (t / seconds) ** curve
    freq = f0 * (f1 / f0) ** k
    phase = 2 * np.pi * np.cumsum(freq) / SR
    return shape(phase)


def tone(freq, seconds, harmonics=(1.0,), vibrato=0.0, vib_rate=5.5):
    t = t_of(seconds)
    f = freq * (1 + vibrato * np.sin(2 * np.pi * vib_rate * t))
    phase = 2 * np.pi * np.cumsum(f) / SR
    out = np.zeros_like(t)
    for i, amp in enumerate(harmonics, start=1):
        out += amp * np.sin(i * phase)
    return out


def bell(freq, seconds, decay=0.6):
    """Колокольчик: неровные обертоны с разным затуханием."""
    t = t_of(seconds)
    out = np.zeros_like(t)
    for ratio, amp, dk in [(1, 1, 1), (2.76, 0.5, 0.6), (5.4, 0.25, 0.35), (8.93, 0.12, 0.2)]:
        out += amp * np.sin(2 * np.pi * freq * ratio * t) * np.exp(-t / (decay * dk))
    return out * np.clip(t / 0.002, 0, 1)


def pluck(freq, seconds, bright=0.5, damp=0.996):
    """Щипок струны (Карплус — Стронг)."""
    n = int(seconds * SR)
    period = max(2, int(SR / freq))
    buf = filt(rng.uniform(-1, 1, period * 4), hi=2000 + 8000 * bright)[:period]
    out = np.empty(n)
    for i in range(n):
        j = i % period
        out[i] = buf[j]
        nxt = buf[(j + 1) % period]
        buf[j] = damp * 0.5 * (buf[j] + nxt)
    return out


def saw(phase):
    return 2 * ((phase / (2 * np.pi)) % 1.0) - 1


def mix(*parts):
    n = max(len(p) for _, p in parts)
    out = np.zeros(n)
    for at, p in parts:
        start = int(at * SR)
        end = min(n, start + len(p))
        if start < n:
            out[start:end] += p[: end - start]
    return out


def place(track, at, part, gain=1.0):
    start = int(at * SR)
    end = min(len(track), start + len(part))
    if start < len(track):
        track[start:end] += gain * part[: end - start]


def reverb(x, seconds=1.2, wet=0.25, seed=0, lowpass=5000):
    ir_rng = np.random.default_rng(seed)
    t = t_of(seconds)
    ir = ir_rng.uniform(-1, 1, len(t)) * np.exp(-t / (seconds / 5))
    ir = filt(ir, hi=lowpass)
    ir /= np.sqrt(np.sum(ir ** 2))
    size = 1 << (len(x) + len(ir)).bit_length()
    tail = np.fft.irfft(np.fft.rfft(x, size) * np.fft.rfft(ir, size), size)[: len(x) + len(ir)]
    dry = np.concatenate([x, np.zeros(len(ir))])
    return dry * (1 - wet) + tail * wet


def finish(x, peak=0.89, fade=0.01):
    x = np.tanh(x / max(1e-9, np.max(np.abs(x))) * 1.2)
    x *= peak / max(1e-9, np.max(np.abs(x)))
    f = int(fade * SR)
    if f and len(x) > 2 * f:
        x[-f:] *= np.linspace(1, 0, f)
    return x


def note(name):
    """'A4' → частота. Бемоль — 'Bb4'."""
    names = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}
    n = names[name[0]]
    rest = name[1:]
    if rest.startswith("b"):
        n -= 1
        rest = rest[1:]
    elif rest.startswith("#"):
        n += 1
        rest = rest[1:]
    midi = 12 * (int(rest) + 1) + n
    return 440 * 2 ** ((midi - 69) / 12)


def save(name, x, stereo=None):
    """Пишет WAV во временную папку и кодирует в OGG."""
    OUT.mkdir(parents=True, exist_ok=True)
    channels = 1 if stereo is None else 2
    data = x if stereo is None else np.stack([x, stereo], axis=1).reshape(-1)
    pcm = (np.clip(data, -1, 1) * 32767).astype("<i2")
    with tempfile.TemporaryDirectory() as tmp:
        wav = Path(tmp) / f"{name}.wav"
        with wave.open(str(wav), "wb") as w:
            w.setnchannels(channels)
            w.setsampwidth(2)
            w.setframerate(SR)
            w.writeframes(pcm.tobytes())
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-i", str(wav),
             "-c:a", "libvorbis", "-q:a", "5", str(OUT / f"{name}.ogg")],
            check=True,
        )
    print("  ", name)


# ---------- звуки боя ----------

def hit(pitch=1.0):
    body = filt(noise(0.09), lo=300 * pitch, hi=2600 * pitch) * env_exp(0.09, 0.025)
    thump = sweep(130 * pitch, 55 * pitch, 0.1) * env_exp(0.1, 0.04)
    return finish(mix((0, body * 0.9), (0, thump * 0.8)), peak=0.8)


def crit():
    base = hit(1.1)
    crack = filt(noise(0.05), lo=2500, hi=9000) * env_exp(0.05, 0.012)
    ring = (np.sin(2 * np.pi * 1850 * t_of(0.25)) + 0.6 * np.sin(2 * np.pi * 2780 * t_of(0.25))) * env_exp(0.25, 0.06)
    return finish(mix((0, base), (0, crack * 0.9), (0.005, ring * 0.35)))


def skill():
    w = filt(noise(0.32), lo=500, hi=4000)
    w *= np.sin(np.linspace(0, np.pi, len(w))) ** 2
    rise = sweep(300, 900, 0.32) * env_adsr(0.32, 0.05, 0.2) * 0.25
    return finish(reverb(mix((0, w), (0, rise)), 0.6, 0.2), peak=0.7)


def ultimate():
    boom = sweep(70, 38, 1.0) * env_exp(1.0, 0.35) + filt(noise(1.0), hi=220) * env_exp(1.0, 0.25) * 1.5
    t = t_of(1.4)
    chord = sum(np.sin(2 * np.pi * note(n) * t * (1 + 0.003 * np.sin(2 * np.pi * 6 * t))) for n in ["A3", "E4", "A4", "B4", "E5"])
    chord *= np.clip(t / 0.35, 0, 1) * np.exp(-np.maximum(t - 0.35, 0) / 0.5) * 0.25
    whoosh = filt(noise(0.5), lo=800, hi=6000) * np.linspace(0, 1, int(0.5 * SR)) ** 2 * 0.4
    return finish(reverb(mix((0, whoosh), (0.4, boom), (0.4, chord)), 1.6, 0.3))


def cannon():
    body = filt(noise(0.8), hi=420) * env_exp(0.8, 0.18) * 1.6
    sub = sweep(55, 32, 0.6) * env_exp(0.6, 0.22)
    crackle = filt(noise(0.12), lo=1800, hi=7000) * env_exp(0.12, 0.03) * 0.5
    return mix((0, body), (0, sub), (0, crackle))


def broadside():
    shots = mix((0, cannon()), (0.13, cannon() * 0.85), (0.27, cannon() * 0.95))
    return finish(reverb(shots, 1.8, 0.35, lowpass=2500))


def heal():
    parts = [(i * 0.07, bell(note(n), 1.0, 0.35) * 0.6) for i, n in enumerate(["C6", "E6", "G6", "C7"])]
    return finish(reverb(mix(*parts), 1.2, 0.35), peak=0.6)


def death():
    fall = filt(sweep(420, 110, 0.35, shape=saw), hi=1500) * env_adsr(0.35, 0.01, 0.15)
    thud = sweep(110, 45, 0.25) * env_exp(0.25, 0.08)
    return finish(mix((0, fall * 0.6), (0.28, thud)), peak=0.7)


def brass(freq, seconds):
    t = t_of(seconds)
    phase = 2 * np.pi * freq * t
    s = saw(phase) + 0.5 * saw(phase * 1.003)
    cutoff_env = 900 + 2600 * np.clip(t / 0.05, 0, 1) * np.exp(-t / 0.6)
    # Фильтр с огибающей: два прохода и смешивание по огибающей.
    bright = filt(s, hi=3500)
    dark = filt(s, hi=900)
    k = (cutoff_env - 900) / 2600
    return (dark * (1 - k) + bright * k) * env_adsr(seconds, 0.02, 0.12)


def victory():
    seq = [("C5", 0.0, 0.14), ("E5", 0.15, 0.14), ("G5", 0.3, 0.14), ("C6", 0.45, 1.2)]
    parts = [(at, brass(note(n), d) * 0.5) for n, at, d in seq]
    chord = sum(brass(note(n), 1.2) for n in ["C4", "G4", "E5"]) * 0.25
    sparkle = bell(note("C7"), 1.0, 0.4) * 0.3
    return finish(reverb(mix(*parts, (0.45, chord), (0.45, sparkle)), 1.6, 0.3))


def defeat():
    seq = [("A4", 0.0, 0.35), ("G4", 0.35, 0.35), ("F4", 0.7, 0.35), ("E4", 1.05, 1.1)]
    parts = [(at, filt(tone(note(n), d, (1, 0.4, 0.2)), hi=1800) * env_adsr(d, 0.04, 0.2) * 0.5) for n, at, d in seq]
    drone = tone(note("A2"), 2.2, (1, 0.5)) * env_adsr(2.2, 0.3, 0.8) * 0.3
    return finish(reverb(mix(*parts, (0, drone)), 1.5, 0.3), peak=0.7)


# ---------- интерфейс ----------

def click():
    tick = filt(noise(0.03), lo=1200, hi=5000) * env_exp(0.03, 0.006)
    knock = np.sin(2 * np.pi * 950 * t_of(0.04)) * env_exp(0.04, 0.012)
    return finish(mix((0, tick), (0, knock * 0.6)), peak=0.55)


def coin():
    return finish(reverb(mix((0, bell(note("E6"), 0.6, 0.25)), (0.08, bell(note("B6"), 0.8, 0.3))), 0.8, 0.25), peak=0.6)


def hammer():
    def knock():
        body = sweep(190, 120, 0.15) * env_exp(0.15, 0.05)
        wood = filt(noise(0.06), lo=500, hi=2500) * env_exp(0.06, 0.015)
        return mix((0, body), (0, wood * 0.8))
    return finish(reverb(mix((0, knock()), (0.18, knock()), (0.36, knock() * 1.1)), 0.6, 0.2), peak=0.75)


def done():
    return finish(reverb(mix((0, bell(note("G5"), 1.6, 0.9)), (0.02, bell(note("D6"), 1.4, 0.7) * 0.4)), 1.4, 0.3), peak=0.65)


def levelup():
    parts = [(i * 0.08, pluck(note(n), 0.6, 0.7) * 0.8) for i, n in enumerate(["C5", "E5", "G5", "C6"])]
    shimmer = bell(note("C7"), 0.9, 0.35) * 0.25
    return finish(reverb(mix(*parts, (0.32, shimmer)), 1.0, 0.3), peak=0.7)


def flip():
    w = filt(noise(0.12), lo=1500, hi=7000)
    w *= np.sin(np.linspace(0, np.pi, len(w))) ** 2
    tick = filt(noise(0.02), lo=2000, hi=6000) * env_exp(0.02, 0.004)
    return finish(mix((0, w * 0.5), (0.1, tick)), peak=0.5)


def ssr():
    t = t_of(1.4)
    cluster = sum(np.sin(2 * np.pi * note(n) * t) for n in ["E6", "G#6", "B6", "E7"])
    cluster *= np.clip(t / 0.3, 0, 1) * np.exp(-t / 0.7) * (0.6 + 0.4 * np.sin(2 * np.pi * 9 * t)) * 0.3
    chord = sum(bell(note(n), 1.4, 0.8) for n in ["E5", "B5", "E6"]) * 0.4
    return finish(reverb(mix((0, cluster), (0.15, chord)), 1.6, 0.35), peak=0.8)


# ---------- музыка ----------

def loopify(dry_l, dry_r, length):
    """Хвост реверберации переносится в начало — петля без шва."""
    n = int(length * SR)
    out = []
    for x in (dry_l, dry_r):
        y = np.zeros(n)
        for i in range(0, len(x), n):
            chunk = x[i : i + n]
            y[: len(chunk)] += chunk
        out.append(y)
    peak = max(np.max(np.abs(out[0])), np.max(np.abs(out[1])))
    return [np.tanh(o / peak * 1.1) * 0.75 for o in out]


def harbor():
    """Спокойная тема корабля: 6/8, ре дорийский, лютня, флейта, бас, бубен."""
    bar = 1.2  # 100 ударов в минуту, такт — две пунктирные четверти
    eighth = bar / 6
    chords = ["Dm", "C", "Bb", "C", "Dm", "F", "C", "Dm"] * 2
    tones = {
        "Dm": ["D3", "A3", "D4", "F4", "A4", "F4"],
        "C": ["C3", "G3", "C4", "E4", "G4", "E4"],
        "Bb": ["Bb2", "F3", "Bb3", "D4", "F4", "D4"],
        "F": ["F2", "C3", "F3", "A3", "C4", "A3"],
    }
    bass = {"Dm": "D2", "C": "C2", "Bb": "Bb1", "F": "F2"}
    melody = [
        [("A4", 3), ("F4", 1), ("E4", 1), ("D4", 1)],
        [("E4", 2), ("G4", 1), ("C5", 3)],
        [("D5", 3), ("C5", 1), ("Bb4", 1), ("A4", 1)],
        [("G4", 4), ("E4", 2)],
        [("A4", 2), ("D5", 1), ("D5", 2), ("E5", 1)],
        [("F5", 3), ("E5", 1), ("D5", 1), ("C5", 1)],
        [("E5", 2), ("D5", 1), ("C5", 2), ("G4", 1)],
        [("D5", 6)],
    ]
    length = bar * len(chords)
    tail = 3.0
    lute = np.zeros(int((length + tail) * SR))
    flute = np.zeros_like(lute)
    low = np.zeros_like(lute)
    drum = np.zeros_like(lute)
    for b, chord in enumerate(chords):
        t0 = b * bar
        for i, n in enumerate(tones[chord]):
            place(lute, t0 + i * eighth + rng.uniform(0, 0.01), pluck(note(n), 0.9, 0.35), 0.5)
        bf = note(bass[chord])
        place(low, t0, filt(tone(bf, bar * 0.95, (1, 0.5, 0.25)), hi=600) * env_adsr(bar * 0.95, 0.02, 0.4), 0.2)
        frame = mix((0, sweep(110, 70, 0.3) * env_exp(0.3, 0.09)),
                    (0, filt(noise(0.05), lo=300, hi=1500) * env_exp(0.05, 0.01) * 0.3))
        place(drum, t0, frame, 0.3)
        place(drum, t0 + 3 * eighth, frame, 0.18)
        for i in range(6):
            shaker = filt(noise(0.04), lo=5000, hi=11000) * env_exp(0.04, 0.012)
            place(drum, t0 + i * eighth, shaker, 0.06 if i % 3 else 0.1)
        # Мелодия — в первой половине; вторая половина — только аккомпанемент.
        if b < 8:
            at = t0
            for n, d in melody[b]:
                dur = d * eighth
                voice = tone(note(n), dur * 1.05, (1, 0.25, 0.08), vibrato=0.004)
                voice *= env_adsr(dur * 1.05, 0.05, min(0.15, dur * 0.5))
                breath = filt(noise(dur * 1.05), lo=1500, hi=5000) * env_adsr(dur * 1.05, 0.03, 0.1) * 0.03
                place(flute, at, voice + breath, 0.5)
                at += dur
    dry = lute + flute + low + drum * 0.9
    left = reverb(dry, 2.0, 0.28, seed=1)
    right = reverb(dry, 2.0, 0.28, seed=2)
    l, r = loopify(left, right, length)
    return l, r


def battle_theme():
    """Тема боя: 140 ударов, ре минор, барабаны, бас-остинато, щипки, медь."""
    beat = 60 / 140
    bar = 4 * beat
    chords = ["Dm", "Dm", "Bb", "C", "Dm", "Dm", "Gm", "A"] * 2
    roots = {"Dm": "D2", "Bb": "Bb1", "C": "C2", "Gm": "G1", "A": "A1"}
    arps = {
        "Dm": ["D4", "F4", "A4", "D5"],
        "Bb": ["Bb3", "D4", "F4", "Bb4"],
        "C": ["C4", "E4", "G4", "C5"],
        "Gm": ["G3", "Bb3", "D4", "G4"],
        "A": ["A3", "C#4", "E4", "A4"],
    }
    lead = [
        None, None, None, None,
        [("D5", 1.5), ("F5", 0.5), ("E5", 1), ("D5", 1)],
        [("A4", 3), ("C5", 1)],
        [("Bb4", 1.5), ("A4", 0.5), ("G4", 1), ("Bb4", 1)],
        [("A4", 4)],
    ]
    length = bar * len(chords)
    tail = 2.5
    n = int((length + tail) * SR)
    drums = np.zeros(n)
    bassl = np.zeros(n)
    strings = np.zeros(n)
    horns = np.zeros(n)

    def taiko(pitch=1.0):
        return mix((0, sweep(80 * pitch, 42 * pitch, 0.45) * env_exp(0.45, 0.13)),
                   (0, filt(noise(0.06), lo=200, hi=1200) * env_exp(0.06, 0.015) * 0.4))

    def snare():
        return mix((0, filt(noise(0.18), lo=1200, hi=6000) * env_exp(0.18, 0.05) * 0.7),
                   (0, sweep(240, 160, 0.1) * env_exp(0.1, 0.03) * 0.5))

    def hat():
        return filt(noise(0.05), lo=6000, hi=12000) * env_exp(0.05, 0.012)

    for b, chord in enumerate(chords):
        t0 = b * bar
        for at, p in [(0, 1.0), (1.5, 1.1), (2, 1.0), (3.5, 1.2)]:
            place(drums, t0 + at * beat, taiko(p), 0.5)
        for at in (1, 3):
            place(drums, t0 + at * beat, snare(), 0.45)
        for i in range(8):
            place(drums, t0 + i * beat / 2, hat(), 0.12 if i % 2 else 0.18)
        if b % 4 == 3:  # сбивка в конце фразы
            for i in range(4):
                place(drums, t0 + 3 * beat + i * beat / 4, snare(), 0.25 + 0.08 * i)
        root = note(roots[chord])
        for i in range(8):
            f = root * (2 if i in (2, 6) else 1)
            d = beat / 2
            v = filt(saw(2 * np.pi * f * t_of(d)) + 0.6 * np.sin(2 * np.pi * f / 2 * t_of(d)), hi=700)
            place(bassl, t0 + i * d, v * env_adsr(d, 0.005, 0.06), 0.22)
        for i in range(16):
            nm = arps[chord][i % 4] if i % 8 < 4 else arps[chord][3 - i % 4]
            place(strings, t0 + i * beat / 4, pluck(note(nm), 0.25, 0.6, damp=0.99), 0.4)
        phrase = lead[b % 8]
        if phrase and b >= 8:
            at = t0
            for nm, d in phrase:
                dur = d * beat
                place(horns, at, brass(note(nm), dur * 0.95), 0.45)
                at += dur
    dry = drums + bassl + strings + horns
    left = reverb(dry, 1.4, 0.2, seed=3)
    right = reverb(dry, 1.4, 0.2, seed=4)
    return loopify(left, right, length)


def main():
    print("Звуки боя:")
    for i, p in enumerate([0.9, 1.0, 1.12], start=1):
        save(f"hit_{i}", hit(p))
    for name, fn in [("crit", crit), ("skill", skill), ("ultimate", ultimate),
                     ("broadside", broadside), ("heal", heal), ("death", death),
                     ("victory", victory), ("defeat", defeat)]:
        save(name, fn())
    print("Интерфейс:")
    for name, fn in [("click", click), ("coin", coin), ("hammer", hammer), ("done", done),
                     ("levelup", levelup), ("flip", flip), ("ssr", ssr)]:
        save(name, fn())
    print("Музыка:")
    l, r = harbor()
    save("music_harbor", l, r)
    l, r = battle_theme()
    save("music_battle", l, r)


if __name__ == "__main__":
    main()
