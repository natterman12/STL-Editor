//! System font files for sketch text (milestone 5, Task 6).
//!
//! The listing itself (1.x `meshtools.font_dirs` / `list_fonts`: `%WINDIR%\Fonts`, the per-user
//! `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, the Linux and Mac font folders, names read with
//! ttf-parser) already lives in `stl_sketch::text::fonts`, and stl-platform has no font parser
//! of its own. The app lists them through `stl_app::sketch::text::system_fonts`. If stl-platform
//! gains a path dependency on stl-sketch, that function can move here.
