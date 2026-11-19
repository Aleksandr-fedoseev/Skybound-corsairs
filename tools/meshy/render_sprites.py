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
from mathutils import Matrix, Vector

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SIZE = 512
MAX_FRAMES = 16
TURN = math.radians(60)   # от взгляда в камеру к профилю вправо: 90° — чистый профиль
UNLIT = 0.6               # доля свечения в материале: 1 — плоская текстура, 0 — обычный свет

args = sys.argv[sys.argv.index("--") + 1:]
NAME = args[0]
OPTS = [a.split("=", 1) for a in args[1:]]
# Особые параметры: turn=<градусы>, prop=<файл.glb>:<кость>:<ось ручки> (можно несколько).
TURN = math.radians(float(next((v for k, v in OPTS if k == "turn"), math.degrees(TURN))))
PROPS = [v.split(":") for k, v in OPTS if k == "prop"]
MAX_FRAMES = int(next((v for k, v in OPTS if k == "frames"), MAX_FRAMES))
CLIPS = {k: v for k, v in OPTS if k not in ("turn", "prop", "frames")}
PROP_LENGTH = 0.36   # длина оружия, м (при росте модели ~1,6 м — стилизованно крупное)
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
    for o in [o for o in bpy.context.scene.objects if o.type == "MESH" and o.parent is not arm
              and o.name.startswith("Icosphere")]:
        bpy.data.objects.remove(o, do_unlink=True)
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    action = arm.animation_data.action if arm.animation_data else None
    return arm, meshes, action


AXES = {"x": Vector((1, 0, 0)), "y": Vector((0, 1, 0)), "z": Vector((0, 0, 1))}


def attach_prop(arm, path, bone_name, grip_axis):
    """Предмет в кулак: ствол — вдоль кости кисти (к пальцам), ручка — вдоль оси кости `grip_axis`
    (x, -x, z, -z). Длинная ось предмета — ствол, ручка — там, где предмет уходит вниз."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=os.path.join(ROOT, path))
    new = [o for o in bpy.data.objects if o not in before]
    prop = next(o for o in new if o.type == "MESH")
    world = prop.matrix_world.copy()
    prop.parent = None
    prop.matrix_world = world
    for o in new:
        if o is not prop:
            bpy.data.objects.remove(o, do_unlink=True)
    vs = [prop.matrix_world @ v.co for v in prop.data.vertices]
    lo = Vector([min(v[i] for v in vs) for i in range(3)])
    hi = Vector([max(v[i] for v in vs) for i in range(3)])
    dims = hi - lo
    order = sorted(range(3), key=lambda i: -dims[i])
    long_i, up_i = order[0], order[1]
    # Ручка — нижняя пятая часть по высоте; её середина по длине — казённая часть.
    low = [v for v in vs if v[up_i] < lo[up_i] + dims[up_i] * 0.2]
    grip_at = sum(v[long_i] for v in low) / len(low)
    mid = (lo[long_i] + hi[long_i]) / 2
    barrel = Vector((0, 0, 0))
    barrel[long_i] = -1.0 if grip_at > mid else 1.0
    down = Vector((0, 0, 0))
    down[up_i] = -1.0
    side = barrel.cross(down)
    # Точка хвата: на ручке, у верха (там кулак).
    grip = Vector((0, 0, 0))
    grip[long_i] = grip_at
    grip[up_i] = lo[up_i] + dims[up_i] * 0.55
    grip[order[2]] = (lo[order[2]] + hi[order[2]]) / 2
    src = Matrix((barrel, down, side)).transposed().to_4x4()
    src.translation = grip

    pb = arm.pose.bones[bone_name]
    bone_world = arm.matrix_world @ pb.matrix
    rot = bone_world.to_3x3().normalized()
    y = (rot @ Vector((0, 1, 0))).normalized()
    sign = -1.0 if grip_axis.startswith("-") else 1.0
    g = (rot @ AXES[grip_axis.lstrip("-")] * sign).normalized()
    g = (g - y * g.dot(y)).normalized()
    dst = Matrix((y, g, y.cross(g))).transposed().to_4x4()
    dst.translation = bone_world.translation + y * 0.07
    scale = PROP_LENGTH / dims[long_i]
    prop.matrix_world = dst @ Matrix.Scale(scale, 4) @ src.inverted()
    final = prop.matrix_world.copy()
    prop.parent = arm
    prop.parent_type = "BONE"
    prop.parent_bone = bone_name
    prop.matrix_world = final
    return prop


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
    if clip == "idle":
        # Длинные ожидания (6–11 с): первые 4 секунды, в игре — туда и обратно (без шва).
        return start, min(end, start + 4 * fps)
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
    for path, bone, axis in PROPS:
        attach_prop(arm, path, bone, axis)
    flatten_materials()
    turn = setup_view(sc, arm, height)
    pelvis = hips(arm)
    start, end = (int(v) for v in action.frame_range) if action else (1, 1)
    start, end = clip_window(sc, arm, clip, start, end)
    limit = MAX_FRAMES * 3 // 2 if clip == "idle" else MAX_FRAMES
    count = min(limit, end - start + 1)
    frames = [round(start + (end - start) * i / max(1, count - 1)) for i in range(count)]
    if clip == "victory" and count > 1:
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
