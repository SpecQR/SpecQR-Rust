use specqr::gs1::*;

#[test]
fn empty_fragment_survives_creation_but_not_normalization() {
    let root = "https://example.com/01/04912345678904";
    let elements = [
        Element::new("01", "04912345678904"),
        Element::new("10", "ABC123"),
        Element::new("17", "251231"),
    ];
    let paths = [
        None,
        Some(vec![]),
        Some(vec!["21".into()]),
        Some(vec!["01".into(), "10".into()]),
    ];
    for (i, path) in paths.into_iter().enumerate() {
        let mut options = DigitalLinkOptions::default().with_base_url("https://example.com#");
        if let Some(path) = path {
            options = options.with_path_ais(path);
        }
        let expected = if i == 0 || i == 3 {
            format!("{root}/10/ABC123?17=251231#")
        } else {
            format!("{root}?10=ABC123&17=251231#")
        };
        let actual = create_digital_link_with_options(&elements, &options).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            normalize_digital_link(&actual).unwrap(),
            format!("{root}/10/ABC123?17=251231")
        );
    }
    let nul = format!("{root}?x=%00");
    assert_eq!(normalize_digital_link(&nul).unwrap(), nul);
}
