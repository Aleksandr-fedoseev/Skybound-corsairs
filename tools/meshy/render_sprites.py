"""Рендер анимированного бойца из Meshy в 2D-кадры для игры.

  blender -b -P tools/meshy/render_sprites.py -- kaya idle=0 attack=232 hit=178 death=189 victory=59

Вход: art/meshy/<имя>/anim_<action_id>.glb (модель со скелетом и одним клипом, скачаны meshy.py).
Выход: art/meshy/<имя>/frames/<клип>/NNNN.png — кадры 512×512 с прозрачным фоном; спрайт-листы
собирает pack_sheets.py.

Персонаж повёрнут на 3/4 вправо (видно лицо), камера ортографическая и одна на все клипы бойца —
ноги стоят в одной точке кадра. Корень клипа не уходит вбок: рывки делает риг в игре.
Вид ближе к иллюстрации: текстура почти без влияния света (подмешано свечение) и тонкий контур.
"""
import math
import os
import sys

import bpy
from mathutils import Vector

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SIZE = 512
MAX_FRAMES = 16
TURN = math.radians(60)   # от взгляда в камеру к профилю вправо: 90° — чистый профиль
UNLIT = 0.6               # доля свечения в материале: 1 — плоская текстура, 0 — обычный свет

args = sys.argv[sys.argv.index("--") + 1:]
NAME = args[0]
CLIPS = dict(a.split("=") for a in args[1:])
SRC = os.path.join(ROOT, "art", "meshy", NAME)


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def configure(sc):
    """Настройки рендера — для активной сцены (glTF-импорт может сделать активной свою)."""
    for engine in ("BLENDER_EEVEE_NEXT", "BLENDER_EEVEE"):
        try:
            sc.render.engine = engine
            break
        except TypeError:
            continue
    sc.render.resolution_x = sc.render.resolution_y = SIZE
    sc.render.film_transparent = True
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    try:
        sc.view_settings.view_transform = "Standard"
    except TypeError:
        pass
    # Тонкий контур, как в рисованном арте.
    sc.render.use_freestyle = False   # на плотной сетке Meshy даёт паутину штрихов; контур — в pack_sheets.py
    sc.render.line_thickness_mode = "ABSOLUTE"
    sc.render.line_thickness = 1.3
    fs = bpy.context.view_layer.freestyle_settings
    lineset = fs.linesets[0] if fs.linesets else fs.linesets.new("lines")
    if lineset.linestyle is None:
        lineset.linestyle = bpy.data.linestyles.new("line")
    lineset.linestyle.color = (0.12, 0.09, 0.09)
    # Только силуэт и края: сгибы на плотной сетке дают сплошную паутину линий.
    lineset.select_by_edge_types = True
    for kind in ("silhouette", "border"):
        setattr(lineset, f"select_{kind}", True)
    for kind in ("crease", "ridge_valley", "suggestive_contour", "material_boundary",
                 "contour", "external_contour", "edge_mark"):
        if hasattr(lineset, f"select_{kind}"):
            setattr(lineset, f"select_{kind}", False)
    world = bpy.data.worlds.new("w")
    sc.world = world
    if world.node_tree is None:
        world.use_nodes = True
    bg = next(n for n in world.node_tree.nodes if n.type == "BACKGROUND")
    bg.inputs["Strength"].default_value = 0.6
    return sc


def flatten_materials():
    """Подмешать к каждому материалу свечение его же базового цвета: меньше влияния света."""
    for mat in bpy.data.materials:
        if not mat.use_nodes:
            continue
        nt = mat.node_tree
        bsdf = next((n for n in nt.nodes if n.type == "BSDF_PRINCIPLED"), None)
        out = next((n for n in nt.nodes if n.type == "OUTPUT_MATERIAL"), None)
        if not bsdf or not out:
            continue
        base = bsdf.inputs["Base Color"]
        emit = nt.nodes.new("ShaderNodeEmission")
        if base.links:
            nt.links.new(base.links[0].from_socket, emit.inputs["Color"])
        else:
            emit.inputs["Color"].default_value = base.default_value
        bsdf.inputs["Roughness"].default_value = 0.85
        mix = nt.nodes.new("ShaderNodeMixShader")
        mix.inputs["Fac"].default_value = UNLIT
        nt.links.new(bsdf.outputs["BSDF"], mix.inputs[1])
        nt.links.new(emit.outputs["Emission"], mix.inputs[2])
        nt.links.new(mix.outputs["Shader"], out.inputs["Surface"])


def import_clip(action_id):
    bpy.ops.import_scene.gltf(filepath=os.path.join(SRC, f"anim_{action_id}.glb"))
    arm = next(o for o in bpy.context.scene.objects if o.type == "ARMATURE")
    # В GLB Meshy бывает служебная сфера без привязки к скелету — она не часть персонажа.
    for o in [o for o in bpy.context.scene.objects if o.type == "MESH" and o.parent is not arm]:
        bpy.data.objects.remove(o, do_unlink=True)
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    action = arm.animation_data.action if arm.animation_data else None
    return arm, meshes, action


def hips(arm):
    """Кость таза (или корневая кость, если таза нет)."""
    bones = arm.pose.bones
    return next((b for b in bones if "hips" in b.name.lower()), None) or next(b for b in bones if b.parent is None)


def setup_view(sc, arm, height):
    root = bpy.data.objects.new("turn", None)
    sc.collection.objects.link(root)
    for o in [o for o in sc.objects if o.parent is None and o is not root]:
        o.parent = root
    root.rotation_euler = (0, 0, TURN)
    cam_data = bpy.data.cameras.new("cam")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = height * 1.5
    cam = bpy.data.objects.new("cam", cam_data)
    sc.collection.objects.link(cam)
    cam.location = (0, -10, height * 0.62)
    cam.rotation_euler = (math.radians(90), 0, 0)
    sc.camera = cam
    for name, rot, energy in (("key", (50, 0, -35), 2.2), ("rim", (60, 0, 150), 1.4)):
        ld = bpy.data.lights.new(name, "SUN")
        ld.energy = energy
        lo = bpy.data.objects.new(name, ld)
        sc.collection.objects.link(lo)
        lo.rotation_euler = tuple(math.radians(v) for v in rot)
    return root


def clip_window(sc, arm, clip, start, end):
    """Атака и умение — окно вокруг пика (кисть быстрее всего): 0,6 с до и 0,4 с после,
    чтобы длинный клип не сжимался в мелькание. Победа — первые 3 секунды."""
    fps = sc.render.fps
    if clip == "victory":
        return start, min(end, start + 3 * fps)
    if clip not in ("attack", "skill"):
        return start, end
    hands = [b for b in arm.pose.bones if "hand" in b.name.lower() and "thumb" not in b.name.lower()
             and "index" not in b.name.lower()]
    best, peak, prev = -1.0, start, None
    for f in range(start, end + 1):
        sc.frame_set(f)
        pos = [(arm.matrix_world @ b.head).copy() for b in hands]
        if prev:
            speed = max((p - q).length for p, q in zip(pos, prev))
            if speed > best:
                best, peak = speed, f
        prev = pos
    return max(start, peak - int(0.6 * fps)), min(end, peak + int(0.4 * fps))


def model_height(meshes):
    """Рост по вычисленной сетке (с позой скелета): рамка скелетной сетки до позы неточна."""
    deps = bpy.context.evaluated_depsgraph_get()
    zs = []
    for o in meshes:
        ev = o.evaluated_get(deps)
        me = ev.to_mesh()
        zs += [(ev.matrix_world @ v.co).z for v in me.vertices]
        ev.to_mesh_clear()
    return max(zs) - min(zs)


def render_clip(clip, action_id, height):
    reset()
    arm, meshes, action = import_clip(action_id)
    sc = configure(bpy.context.scene)
    flatten_materials()
    turn = setup_view(sc, arm, height)
    pelvis = hips(arm)
    start, end = (int(v) for v in action.frame_range) if action else (1, 1)
    start, end = clip_window(sc, arm, clip, start, end)
    count = min(MAX_FRAMES, end - start + 1)
    frames = [round(start + (end - start) * i / max(1, count - 1)) for i in range(count)]
    if clip in ("idle", "victory") and count > 1:
        # Зацикленные: последний кадр совпадает с первым — не дублируем.
        frames = [round(start + (end - start) * i / count) for i in range(count)]
    out = os.path.join(SRC, "frames", clip)
    os.makedirs(out, exist_ok=True)
    for old in os.listdir(out):
        os.remove(os.path.join(out, old))
    x0 = None
    for i, f in enumerate(frames):
        # Клип на месте: таз держим в одной точке по горизонтали экрана (X), глубина не видна.
        turn.location = (0, 0, 0)
        sc.frame_set(f)
        x = (arm.matrix_world @ pelvis.head).x
        x0 = x if x0 is None else x0
        turn.location = (x0 - x, 0, 0)
        bpy.context.view_layer.update()
        sc.render.filepath = os.path.join(out, f"{i:04d}.png")
        bpy.ops.render.render(write_still=True)
    fps = sc.render.fps / max(1, (end - start) / max(1, count - 1))
    print(f"[render_sprites] {NAME} {clip}: {count} кадров из {start}–{end} (клип {action.frame_range[0]:.0f}–{action.frame_range[1]:.0f}), ~{fps:.1f} к/с", flush=True)


# Высота по позе покоя первого клипа — общая камера для всех клипов.
reset()
_, first_meshes, _ = import_clip(next(iter(CLIPS.values())))
HEIGHT = model_height(first_meshes)
print(f"[render_sprites] {NAME}: высота модели {HEIGHT:.2f} м", flush=True)
for clip, action_id in CLIPS.items():
    render_clip(clip, action_id, HEIGHT)
