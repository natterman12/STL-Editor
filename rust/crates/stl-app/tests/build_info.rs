//! Task 5: the window title and About say which test build this is.

#[test]
fn title_says_test_build() {
    let t = stl_app::build_info::title();
    assert!(t.starts_with("STL Editor 2.0 (test build "), "{t}");
    assert!(t.ends_with(')'), "{t}");
    let inner = t.trim_start_matches("STL Editor 2.0 (test build ").trim_end_matches(')');
    let (date, commit) = inner.split_once(' ').expect("date and commit");
    let d: Vec<&str> = date.split('-').collect();
    assert_eq!(d.len(), 3, "{date}");
    assert!(d[0].len() == 4 && d[1].len() == 2 && d[2].len() == 2, "{date}");
    assert!(d.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())), "{date}");
    assert!(!commit.is_empty() && !commit.contains(' '), "{commit}");
    assert_eq!(stl_app::build_info::build_date(), date);
    assert_eq!(stl_app::build_info::commit(), commit);
}
