"""
The case for the two radars: the LD2450 stands upright on the left (its plug end at the
top, the way Hi-Link says to mount it), the LD2410C sits on the right with its pins
pointing down. Both look out through open windows, so nothing slows the radio waves.
Wires leave through a slot at the bottom of the back; the back lid presses in and has a
keyhole for hanging it on a screw 1.5-2 m up a wall.

    uv run --with manifold3d --with numpy --with matplotlib case/case.py

writes case/body.stl, case/lid.stl and case/preview.png. Everything is in mm.
x is left-right seen from the front, y goes from the front face to the back, z is up.
"""

from pathlib import Path

import numpy as np
from manifold3d import Manifold

HERE = Path(__file__).parent

WALL = 2.0  # sides, top, bottom
FRONT = 2.0  # front face the radars sit behind
FIT = 0.4  # extra room around each board so it drops in
RIM = 1.2  # how far the front face overlaps the board edge, holding it in

# boards: (width along x, height along z), measured from the datasheets
LD2450 = (15.0, 44.0)  # upright, plug end at the top
LD2410C = (22.0, 16.0)  # pins along the bottom edge, pointing down
BOARD_DEPTH = 1.8  # pocket depth behind the front face: pcb plus a little

INNER_W = 54.0
INNER_H = 70.0
INNER_D = 20.0  # room behind the boards for the plug and wires to bend back
LID = 2.0
LIP = 3.0  # how deep the lid's lip reaches into the case
LIP_FIT = 0.25

OUTER_W = INNER_W + 2 * WALL
OUTER_H = INNER_H + 2 * WALL
OUTER_D = FRONT + BOARD_DEPTH + INNER_D

# where each board sits, measured from the inside bottom-left corner
LD2450_AT = (4.0, 8.0)  # 8 mm below for its factory pins, 18 mm above for the plug
LD2410C_AT = (4.0 + LD2450[0] + FIT + 8.0, 8.0 + LD2450[1] + FIT - LD2410C[1] - FIT)


def box(x, y, z, size):
    return Manifold.cube(size).translate((x, y, z))


def flared_window(x, z, w, h, depth, flare):
    """A window through the front face that widens toward the outside by `flare` on
    every side, so the edges don't block the radar's wide view."""
    inner = Manifold.cube((w, 0.01, h)).translate((x, depth - 0.01, z))
    outer = Manifold.cube((w + 2 * flare, 0.01, h + 2 * flare)).translate((x - flare, -0.01, z - flare))
    return Manifold.batch_hull([inner, outer])


def board_seat(at, board, open_above=0.0, open_below=0.0):
    """The pocket a board drops into from behind, and the window in front of it.

    Plugs and pins that stick out past a board's edge need a channel there, or the
    frame around the pocket stops the board sitting flat. The channels are narrower
    than the board, so its corners still rest on a ledge."""
    x = WALL + at[0]
    z = WALL + at[1]
    w = board[0] + FIT
    h = board[1] + FIT
    pocket = box(x, FRONT, z, (w, BOARD_DEPTH + 0.01, h))
    if open_above:
        pocket += box(x + 1.5, FRONT, z + h - 0.01, (w - 3, BOARD_DEPTH + 0.01, open_above))
    if open_below:
        pocket += box(x + 1.5, FRONT, z - open_below + 0.01, (w - 3, BOARD_DEPTH + 0.01, open_below))
    # the front face is thinner around the windows so the flare can be generous
    flare = FRONT * np.tan(np.radians(60))
    window = flared_window(x + RIM, z + RIM, w - 2 * RIM, h - 2 * RIM, FRONT, flare)
    return pocket + window


def body():
    shell = box(0, 0, 0, (OUTER_W, OUTER_D, OUTER_H))
    hollow = box(WALL, FRONT + BOARD_DEPTH, WALL, (INNER_W, INNER_D + 0.01, INNER_H))
    case = shell - hollow
    # the LD2450's plug sticks out of its top edge, and its factory pins out of the bottom
    case -= board_seat(LD2450_AT, LD2450, open_above=12, open_below=LD2450_AT[1])
    # the LD2410C's pins and their plugs hang below it
    case -= board_seat(LD2410C_AT, LD2410C, open_below=30)
    # the wires leave through a slot in the bottom, open toward the back
    slot_w, slot_h = 16.0, 8.0
    case -= box(OUTER_W / 2 - slot_w / 2, OUTER_D - slot_h, -0.01, (slot_w, slot_h + 0.01, WALL + 0.02))
    return case


def lid():
    """Lies flat for printing: the plate on the bed, the lip pointing up."""
    plate = box(0, 0, 0, (OUTER_W, OUTER_H, LID))
    lip_w = INNER_W - 2 * LIP_FIT
    lip_h = INNER_H - 2 * LIP_FIT
    lip = box(WALL + LIP_FIT, WALL + LIP_FIT, LID, (lip_w, lip_h, LIP))
    lip -= box(WALL + LIP_FIT + 1.2, WALL + LIP_FIT + 1.2, LID - 0.01, (lip_w - 2.4, lip_h - 2.4, LIP + 0.02))
    # leave room in the lip where the wire slot is (the slot sits at the bottom edge)
    lip -= box(OUTER_W / 2 - 9, 0, LID - 0.01, (18, WALL + 4, LIP + 0.02))
    part = plate + lip
    # keyhole near the top: push the screw head through the big hole, slide down
    kx, ky = OUTER_W / 2, OUTER_H - 16
    part -= Manifold.cylinder(LID * 3, 4.5, circular_segments=48).translate((kx, ky, -LID))
    part -= box(kx - 1.6, ky - 9, -LID, (3.2, 9, LID * 3))
    part -= Manifold.cylinder(LID * 3, 1.6, circular_segments=24).translate((kx, ky - 9, -LID))
    # a notch to get a fingernail under it
    part -= box(OUTER_W / 2 - 6, OUTER_H - 1.2, -0.01, (12, 1.21, LID + 0.02))
    return part


def write_stl(manifold, path):
    mesh = manifold.to_mesh()
    verts = np.asarray(mesh.vert_properties)[:, :3].astype(np.float32)
    tris = np.asarray(mesh.tri_verts)
    a, b, c = verts[tris[:, 0]], verts[tris[:, 1]], verts[tris[:, 2]]
    normals = np.cross(b - a, c - a)
    normals /= np.linalg.norm(normals, axis=1, keepdims=True) + 1e-12
    record = np.zeros(len(tris), dtype=[("n", "<f4", 3), ("v", "<f4", (3, 3)), ("attr", "<u2")])
    record["n"] = normals
    record["v"] = np.stack([a, b, c], axis=1)
    with open(path, "wb") as f:
        f.write(b"radar-detection case".ljust(80, b" "))
        f.write(np.uint32(len(tris)).tobytes())
        f.write(record.tobytes())


def preview(parts, path):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from mpl_toolkits.mplot3d.art3d import Poly3DCollection

    fig = plt.figure(figsize=(12, 6), facecolor="#0b0f10")
    views = [("front", 15, -65), ("back", 20, 120)]
    for i, (title, elev, azim) in enumerate(views):
        ax = fig.add_subplot(1, 2, i + 1, projection="3d", facecolor="#0b0f10")
        for part, color in parts:
            mesh = part.to_mesh()
            verts = np.asarray(mesh.vert_properties)[:, :3]
            faces = verts[np.asarray(mesh.tri_verts)]
            ax.add_collection3d(Poly3DCollection(faces, facecolor=color, edgecolor="#00000022", linewidths=0.2))
        ax.set_xlim(-10, 70)
        ax.set_ylim(-20, 60)
        ax.set_zlim(-2, 78)
        ax.set_box_aspect((1, 1, 1))
        ax.view_init(elev=elev, azim=azim)
        ax.set_axis_off()
        ax.set_title(title, color="#d6fff0")
    fig.tight_layout()
    fig.savefig(path, dpi=110)


if __name__ == "__main__":
    b = body()
    l = lid()
    for name, part in (("body", b), ("lid", l)):
        assert part.status().name == "NoError", part.status()
        write_stl(part, HERE / f"{name}.stl")
    # the lid shown in place on the back, a little pulled out
    placed = l.rotate((90, 0, 0)).translate((0, OUTER_D + 8 + LID, 0))
    preview([(b, "#3fcf9f"), (placed, "#2a6f5a")], HERE / "preview.png")
    print(f"case: {OUTER_W:.0f} x {OUTER_H:.0f} x {OUTER_D:.0f} mm (w x h x d)")
