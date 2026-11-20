use ps5scaler::lut::*;

const IDENTITY_2: &str =
    "# identity\nTITLE \"id\"\nLUT_3D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1.0 1.0 1.0\n\n\
    0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

#[test]
fn parses_identity_with_red_fastest() {
    let cube = parse_cube(IDENTITY_2).unwrap();
    assert_eq!(cube.size, 2);
    assert_eq!(cube.data.len(), 8);
    assert_eq!(cube.data[1], [1.0, 0.0, 0.0]);
    assert_eq!(cube.data[4], [0.0, 0.0, 1.0]);
    assert_eq!(cube.domain_max, [1.0; 3]);
}

#[test]
fn reads_domain_and_ignores_unknown_keywords() {
    let text = IDENTITY_2.replace(
        "DOMAIN_MAX 1.0 1.0 1.0",
        "DOMAIN_MAX 2 2 2\nLUT_3D_INPUT_RANGE 0 1",
    );
    assert_eq!(parse_cube(&text).unwrap().domain_max, [2.0; 3]);
}

#[test]
fn rejects_bad_files() {
    assert!(parse_cube("0 0 0\n").unwrap_err().contains("LUT_3D_SIZE"));
    assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n")
        .unwrap_err()
        .contains("8 rows"));
    assert!(parse_cube("LUT_3D_SIZE 2\n0 0 x\n")
        .unwrap_err()
        .contains("line 2"));
    assert!(parse_cube("LUT_1D_SIZE 16\n").is_err());
}
