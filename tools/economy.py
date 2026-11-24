#!/usr/bin/env python3
"""Экономика по дням: модель игрока, который заходит в игру несколько раз в день.

  python3 tools/economy.py [дней]        # по умолчанию 14

Читает data/*.json, сложность этапов — их recommended_level, конвоев — recommended_level
уровня конвоя, этажей Бездны — таблица из `cargo run -p game-data --example curve -- 100 abyss`
(кэш в target/abyss_levels.json; удалите его после правок Бездны или героев).

Упрощения: отряд — 4 героя, качается самый слабый; уровень отряда — среднее (как на карте);
звёзды, умения, снаряжение, найм, ивенты и экспедиции не учитываются; игрок жадный:
строит по списку, качает героев на всё, фармит этапы на все припасы.
"""
import json
import math
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "data"
SESSIONS = [8, 13, 19, 23]          # часы заходов в игру
TEAM = 4
LEVEL_CAP = 20                      # потолок уровня на 1★


def load(name):
    return json.loads((DATA / name).read_text())


menu, supply, crew = load("menu.json"), load("supplies.json"), load("crew.json")
stages, convoys, contracts = load("stages.json"), load("convoys.json"), load("contracts.json")
abyss, ship = load("abyss.json"), load("ship.json")
comp = {c["id"]: c for c in ship["compartments"]}


def abyss_levels():
    cache = ROOT / "target/abyss_levels.json"
    if cache.exists():
        return {int(k): v for k, v in json.loads(cache.read_text()).items()}
    out = subprocess.run(
        ["cargo", "run", "-q", "-p", "game-data", "--example", "curve", "--release", "--", "100", "abyss"],
        cwd=ROOT, capture_output=True, text=True, check=True).stdout
    table = {}
    for line in out.splitlines():
        if line.startswith("Бездна"):
            parts = line.split()
            table[int(parts[2])] = int(parts[4].lstrip(">"))
    cache.write_text(json.dumps(table))
    return table


ABYSS = abyss_levels()


def level_cost(level):
    steps = crew["journal_steps"]
    for (l0, j0), (l1, j1) in zip(steps, steps[1:]):
        if level <= l1:
            t = (max(level, l0) - l0) / (l1 - l0)
            return round(j0 * (j1 / j0) ** t), crew["silver_per_level"] * level
    return round(steps[-1][1]), crew["silver_per_level"] * level


def effect_at(cid, level):
    e = comp[cid].get("effect") or {}
    if level == 0:
        return 0 if e.get("kind") != "hold" else None
    vals = e.get("values")
    return vals[min(level, len(vals)) - 1] if vals else None


def build_cost(cid, to):
    c = dict(next(l for l in ship["levels"] if l["level"] == to))
    return c


class Player:
    def __init__(self):
        s = menu["start"]
        self.silver, self.planks, self.ether = s["silver"], s["planks"], s["ether"]
        self.doubloons, self.journals = s["doubloons"], s["journals"]
        self.supplies, self.regen, self.galley = s["supplies"], 0.0, 0.0
        self.ether_pending = s["ether_waiting"]
        self.heroes = [1] * TEAM
        self.cleared, self.wins_today, self.best_floor = 0, {}, 0
        self.convoys = convoys["start_ready"]
        self.convoy_clock = 0.0
        self.ship = {c["id"]: c["start_level"] for c in ship["compartments"]}
        self.vessel = ship["ships"][0]          # корабль: ранг открывает отсеки, max_level — их потолок
        self.build = None               # (id, to, ends_at_hours)
        self.deeds = {}
        self.claimed = set()
        self.log = {}                   # статистика дня
        self.cleared_at = {}

    # --- вместимость и доход -------------------------------------------------
    def cap(self):
        e = comp["hold"]["effect"]
        lv = self.ship["hold"]
        return {k: e[k][min(lv, len(e[k])) - 1] for k in ("silver", "planks", "ether")}

    def give(self, silver=0, planks=0, journals=0, src="бои"):
        cap = self.cap()
        self.silver = min(self.silver + silver, cap["silver"])
        self.planks = min(self.planks + planks, cap["planks"])
        self.journals += journals
        self.add("журналы: " + src, journals)

    def add(self, key, n):
        self.log[key] = self.log.get(key, 0) + n

    def squad(self):
        return sum(self.heroes) // TEAM

    def pass_time(self, hours, now):
        minutes = hours * 60
        # припасы
        if self.supplies < supply["cap"]:
            self.regen += minutes / supply["regen_minutes"]
            whole = math.floor(self.regen)
            self.regen -= whole
            gained = min(whole, supply["cap"] - self.supplies)
            self.supplies += gained
            self.add("припасы: восстановление", gained)
        else:
            self.add("припасы: потеряно на потолке", minutes / supply["regen_minutes"])
        galley = effect_at("galley", self.ship["galley"]) or 0
        self.galley = min(self.galley + galley * hours, galley * supply["galley_hours"])
        # журналы Тренировочной палубы — сразу в запас
        jph = effect_at("training_deck", self.ship["training_deck"]) or 0
        self.journals += jph * hours
        self.add("журналы: Тренировочная палуба", jph * hours)
        epm = effect_at("windcatchers", self.ship["windcatchers"]) or 0
        self.ether_pending = min(self.ether_pending + epm * minutes, self.cap()["ether"])
        # конвои
        self.convoy_clock += minutes
        while self.convoy_clock >= convoys["every_minutes"]:
            self.convoy_clock -= convoys["every_minutes"]
            self.convoys = min(self.convoys + 1, convoys["max_ready"])
        # стройка
        if self.build and now >= self.build[2]:
            cid, to, _ = self.build
            self.ship[cid] = to
            self.build = None

    # --- заход в игру ----------------------------------------------------------
    def session(self, now, day):
        self.ether = min(self.ether + int(self.ether_pending), self.cap()["ether"])
        self.ether_pending -= int(self.ether_pending)
        self.deed("ether")
        got = int(self.galley)
        self.supplies += got
        self.galley -= got
        self.add("припасы: Камбуз", got)
        self.try_build(now)
        self.fight(day, now)
        self.claim()
        self.level_up()
        self.try_build(now)

    def deed(self, kind, n=1):
        self.deeds[kind] = self.deeds.get(kind, 0) + n

    def claim(self):
        for c in contracts["daily"]:
            if c["id"] not in self.claimed and self.deeds.get(c["deed"], 0) >= c["need"]:
                self.claimed.add(c["id"])
                self.reward(c["reward"])
        if len(self.claimed) == len(contracts["daily"]) and "all" not in self.claimed:
            self.claimed.add("all")
            self.reward(contracts["all_done"])

    def reward(self, r):
        self.give(r.get("silver", 0), r.get("planks", 0), r.get("journals", 0), "контракты")
        self.doubloons += r.get("doubloons", 0)
        self.supplies += r.get("supplies", 0)
        self.add("припасы: контракты", r.get("supplies", 0))

    def stage_win(self, i, day, now):
        wins = self.wins_today.get(i, 0)
        pct = supply["replay_percent"][wins] / 100
        r = menu["victory_reward"]
        self.give(int(r["silver"] * pct), int(r["planks"] * pct), int(r["journals"] * pct))
        self.wins_today[i] = wins + 1
        # Первая победа на новом этапе — без припасов (если так в data/supplies.json).
        cost = 0 if i == self.cleared and supply.get("first_clear_free") else supply["cost"]
        self.supplies -= cost
        self.add("припасы: потрачено", cost)
        self.add("побед на этапах", 1)
        self.deed("win")
        if i == self.cleared:
            self.cleared += 1
            self.doubloons += menu["first_clear_doubloons"]
            self.cleared_at[stages[i]["id"]] = f"день {day + 1}, {now % 24:02.0f}:00"

    def can_stage(self, i):
        free = i == self.cleared and supply.get("first_clear_free")
        return (self.squad() >= stages[i]["recommended_level"]
                and (free or self.supplies >= supply["cost"])
                and self.wins_today.get(i, 0) < len(supply["replay_percent"]))

    def fight(self, day, now):
        # новые этапы
        while self.cleared < len(stages) and self.can_stage(self.cleared):
            self.stage_win(self.cleared, day, now)
        # конвои
        tier = [t for t in convoys["tiers"] if t["from_cleared"] <= self.cleared][-1]
        while self.convoys > 0 and self.squad() >= tier["recommended_level"]:
            self.convoys -= 1
            k = 2 if day % 7 in (5, 6) else 1
            self.give(tier["silver"] * k, tier["planks"] * k, 0)
            self.add("конвоев взято", 1)
            self.deed("win")
            self.deed("convoy")
        # Бездна — награда один раз за этаж
        while ABYSS.get(self.best_floor + 1, 999) <= self.squad():
            f = self.best_floor = self.best_floor + 1
            self.give(abyss["silver"] + abyss["silver_per_floor"] * f, 0,
                      abyss["journals"] + abyss["journals_per_floor"] * f, "Бездна")
            if f % abyss["boss_every"] == 0:
                self.doubloons += abyss["boss_doubloons"]
            self.deed("win")
        # фарм пройденного на остаток припасов: этап с наименьшим числом побед сегодня
        while True:
            options = [i for i in range(self.cleared) if self.can_stage(i)]
            if not options:
                break
            self.stage_win(min(options, key=lambda i: self.wins_today.get(i, 0)), day, now)

    def level_up(self):
        while True:
            i = min(range(TEAM), key=lambda k: self.heroes[k])
            lv = self.heroes[i]
            j, s = level_cost(lv)
            if lv >= LEVEL_CAP or self.journals < j or self.silver < s:
                break
            self.journals -= j
            self.silver -= s
            self.heroes[i] += 1
            self.deed("levelup")

    def chapters_done(self):
        done = 0
        for n in range(1, 20):
            idx = [i for i, st in enumerate(stages) if st["id"].startswith(f"{n}-")]
            if not idx or idx[-1] >= self.cleared:
                break
            done = n
        return done

    def wish(self):
        """Что строить: Тренировка, Трюм, Камбуз, Пушки, Ветроловы до потолка корабля; потом новый корабль."""
        cap, rank = self.vessel["max_level"], self.vessel["rank"]
        for cid in ("training_deck", "hold", "galley", "cannon_deck", "windcatchers"):
            if comp[cid]["rank"] <= rank and self.ship[cid] < cap:
                return cid, self.ship[cid] + 1
        # Новый корабль с Верфи (за дублоны не берём) — когда открыт по главам.
        nxt = next((c for c in ship["ships"] if not c.get("doubloons") and c["rank"] > rank), None)
        if nxt and nxt.get("after_chapter", 0) <= self.chapters_done():
            return "ship", nxt
        return None

    def try_build(self, now):
        if self.build:
            return
        w = self.wish()
        if not w:
            return
        cid, to = w
        if cid == "ship":
            price = {"silver": 0, "planks": 0, "ether": 0, **to.get("price", {})}
            if self.silver >= price["silver"] and self.planks >= price["planks"] and self.ether >= price["ether"]:
                self.silver -= price["silver"]
                self.planks -= price["planks"]
                self.ether -= price["ether"]
                self.vessel = to  # покупка мгновенная, отсеки переезжают
            return
        c = build_cost(cid, to)
        if self.silver >= c["silver"] and self.planks >= c["planks"] and self.ether >= c["ether"]:
            self.silver -= c["silver"]
            self.planks -= c["planks"]
            self.ether -= c["ether"]
            self.build = (cid, to, now + c["seconds"] / 3600)
            self.deed("build")


def main():
    days = int(sys.argv[1]) if len(sys.argv) > 1 else 14
    p = Player()
    now = SESSIONS[0]
    print(f"Заходы: {SESSIONS} ч · этапов {len(stages)} · потолок уровня {LEVEL_CAP} (1★)\n")
    print(f"{'день':>4} {'отряд':>5} {'этапы':>5} {'Бездна':>6} {'побед':>5} {'журн. бои/трен/контр/Бездна':>28}"
          f" {'припасы трат/потер':>18} {'Ранг':>6} {'Трен.':>5}")
    for day in range(days):
        p.log, p.wins_today, p.deeds, p.claimed = {}, {}, {}, set()
        for h in SESSIONS:
            t = day * 24 + h
            # время между заходами (стройка заканчивается по часам внутри промежутка)
            while now < t:
                step = min(1.0, t - now)
                now += step
                p.pass_time(step, now)
            p.session(now, day)
        L = p.log
        jr = "/".join(str(int(L.get("журналы: " + k, 0))) for k in ("бои", "Тренировочная палуба", "контракты", "Бездна"))
        print(f"{day + 1:>4} {p.squad():>5} {p.cleared:>5} {p.best_floor:>6} {int(L.get('побед на этапах', 0)):>5}"
              f" {jr:>28} {int(L.get('припасы: потрачено', 0)):>8}/{int(L.get('припасы: потеряно на потолке', 0)):<9}"
              f" {p.vessel['rank']:>6} {p.ship['training_deck']:>5}")
    print("\nПервое прохождение:")
    for sid, when in p.cleared_at.items():
        rec = next(s["recommended_level"] for s in stages if s["id"] == sid)
        print(f"  {sid:<7} (ур. {rec:>2}) — {when}")
    left = [s["id"] for s in stages[p.cleared:]]
    if left:
        print(f"  не пройдены за {days} дн.: {', '.join(left)}")
    print(f"\nГерои: {p.heroes} · журналов в запасе {p.journals:.0f} · серебро {p.silver} · дублоны {p.doubloons}")


if __name__ == "__main__":
    main()
