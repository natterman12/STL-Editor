//! Task 5: the window title and About say which test build this is.

/// A released build (no `-dev` on the version) does not call itself a test build, in the title or
/// in About, and About still says when it was built and from which commit.
#[test]
fn a_released_version_is_not_called_a_test_build() {
    use stl_app::build_info::{build_date, commit, is_test_version, title_for, version_line_for};
    for v in ["2.0.0", "2.0.0-beta.1", "2.1.3"] {
        assert!(!is_test_version(v), "{v}");
        assert_eq!(title_for(v), format!("STL Editor {v}"));
        let line = version_line_for(v);
        assert!(!line.contains("test build"), "{line}");
        assert_eq!(line, format!("Version {v} (built {}, {})", build_date(), commit()));
    }
    assert!(is_test_version("2.0.0-dev"));
    assert!(version_line_for("2.0.0-dev").starts_with("Version 2.0.0-dev (test build "));
}

/// Both editions say the program is free software and where its source is.
#[test]
fn about_names_the_licence_on_the_desktop_too() {
    for web in [false, true] {
        let text = stl_app::app::about_text(web);
        assert!(
            text.contains("Free software under the GPL-3.0: source on GitHub."),
            "web={web}: {text}"
        );
    }
}

#[test]
fn title_says_test_build() {
    // only between releases: a released build's title is checked above
    if !stl_app::build_info::is_test_version(stl_app::build_info::version()) {
        return;
    }
    let t = stl_app::build_info::title();
    assert!(t.starts_with("STL Editor 2.0 (test build "), "{t}");
    assert!(t.ends_with(')'), "{t}");
    let inner = t
        .trim_start_matches("STL Editor 2.0 (test build ")
        .trim_end_matches(')');
    let (date, commit) = inner.split_once(' ').expect("date and commit");
    let d: Vec<&str> = date.split('-').collect();
    assert_eq!(d.len(), 3, "{date}");
    assert!(d[0].len() == 4 && d[1].len() == 2 && d[2].len() == 2, "{date}");
    assert!(d.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())), "{date}");
    assert!(!commit.is_empty() && !commit.contains(' '), "{commit}");
    assert_eq!(stl_app::build_info::build_date(), date);
    assert_eq!(stl_app::build_info::commit(), commit);
}
