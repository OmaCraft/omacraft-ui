//! Print what the font resolver finds on this machine. Used to check real hosts (OmarchyNuc,
//! OmarchyMax, macOS, Windows) without starting an app.

fn main() {
    let faces = omacraft_ui::SystemFaces::detect();
    println!("system UI stack (after Inter):");
    for face in &faces.ui {
        println!("  {} -> {}#{}", face.family, face.path.display(), face.index);
    }
    println!("system UI bold:");
    for face in &faces.ui_bold {
        println!("  {} -> {}#{}", face.family, face.path.display(), face.index);
    }
    match &faces.mono {
        Some(face) => println!("mono: {} -> {}", face.family, face.path.display()),
        None => println!("mono: none (bundled fallback)"),
    }
}
