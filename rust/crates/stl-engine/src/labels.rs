/// Feature type -> the word a new feature is named after ("Extrude1"). Copied from 1.x
/// (`LABELS` in `stleditor/features.py`, then the `feature_modules` in the order they load).
pub const LABELS: [(&str, &str); 37] = [
    ("import", "Import"),
    ("primitive", "Primitive"),
    ("plane", "Plane"),
    ("sketch", "Sketch"),
    ("extrude", "Extrude"),
    ("revolve", "Revolve"),
    ("loft", "Loft"),
    ("sweep", "Sweep"),
    ("hole", "Hole"),
    ("combine", "Combine"),
    ("split", "Split"),
    ("mirror", "Mirror"),
    ("move", "Move"),
    ("scale", "Scale"),
    ("hollow", "Hollow"),
    ("shell", "Shell"),
    ("repair", "Repair"),
    ("simplify", "Simplify"),
    ("pattern", "Pattern"),
    ("delete", "Remove"),
    ("separate", "Separate"),
    ("bedsplit", "Fit to Bed"),
    ("pushpull", "Push/Pull"),
    ("sculpt", "Sculpt"),
    ("remesh", "Remesh"),
    ("layflat", "Lay Flat"),
    ("plate", "Plate Layout"),
    ("thread", "Thread"),
    ("offset", "Offset"),
    ("resizehole", "Resize Hole"),
    ("supportfins", "Support Fins"),
    ("supportwall", "Draw Walls"),
    ("thincheck", "Thin-wall Check"),
    ("fittest", "Fit Test"),
    ("lattice", "Lattice"),
    ("snapfit", "Snap-fit"),
    ("hinge", "Living hinge"),
];

/// The label of a feature type, or the type itself when it has none.
pub fn label(kind: &str) -> &str {
    LABELS.iter().find(|(k, _)| *k == kind).map_or(kind, |(_, l)| l)
}
