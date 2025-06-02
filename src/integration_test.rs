use crate::Hll;
use flate2::read::GzDecoder;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::Path,
};

#[derive(Debug)]
struct AddTestCase {
    hll: Hll,
    to_add: u64,
    cardinality: u64,
}

#[derive(Debug)]
struct UnionTestCase {
    hll: Hll,
    to_union: Hll,
    cardinality: u64,
}

#[test]
fn integration_test_suite() {
    let integration_tests_dir = Path::new("integration_tests");
    let suites =
        fs::read_dir(integration_tests_dir).expect("Failed to read integration_tests directory");

    for suite in suites {
        let suite = suite.expect("Failed to read suite directory");
        let suite_dir = integration_tests_dir.join(suite.file_name());
        let files: Result<Vec<_>, _> = fs::read_dir(&suite_dir)
            .expect("Failed to read suite directory")
            .collect();

        let files = files.expect("Failed to read test files");

        files.into_par_iter().for_each(|file| {
            integration_test(&file, &suite_dir);
        });
    }
}

fn integration_test(file: &fs::DirEntry, suite_dir: &Path) {
    let test_path = suite_dir.join(file.file_name());
    let test_name = test_path.to_string_lossy().to_string();
    let file = File::open(&test_path).expect("Failed to open test file");
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);
    let mut lines = reader.lines();

    // Skip header
    lines
        .next()
        .expect("Failed to read header")
        .expect("Invalid header");

    if test_name.contains("_add_") {
        let mut hll = None;
        let mut line_no = 2; // line 1 was discarded above

        for line in lines {
            let line = line.expect("Failed to read line");
            let tt = parse_add_test_case(&line, line_no);

            if hll.is_none() {
                hll = Some(tt.hll.clone());
            }

            if let Some(ref mut h) = hll {
                h.add_raw(tt.to_add);

                // Check storage type
                assert_eq!(
                    std::mem::discriminant(&tt.hll),
                    std::mem::discriminant(h),
                    "wrong storage type at line {} {test_name}",
                    line_no
                );

                assert_eq!(&tt.hll, h, "invalid state at line {} {test_name}", line_no);

                // Check cardinality
                assert_eq!(
                    tt.cardinality,
                    h.cardinality(),
                    "incorrect cardinality at line {} {test_name}, hll: \\x{}",
                    line_no,
                    hex::encode(h.to_bytes())
                );

                // Check serialized value
                assert_eq!(
                    hex::encode(tt.hll.to_bytes()),
                    hex::encode(h.to_bytes()),
                    "incorrect serialized value at line {} {test_name}",
                    line_no
                );
            }

            line_no += 1;
        }
    } else {
        let mut hll = None;
        let mut line_no = 2; // line 1 was discarded above

        for line in lines {
            let line = line.expect("Failed to read line");
            let tt = parse_union_test_case(&line, line_no);

            if hll.is_none() {
                hll = Some(tt.hll.clone());
            }

            if let Some(ref mut h) = hll {
                h.union(true, &tt.to_union).expect("Failed to union HLLs");

                // Check cardinality
                assert_eq!(
                    tt.cardinality,
                    h.cardinality(),
                    "incorrect cardinality at line {} {test_name}, hll: \\x{}",
                    line_no,
                    hex::encode(h.to_bytes())
                );

                // Check serialized value if storage types match
                if std::mem::discriminant(&tt.hll) == std::mem::discriminant(h) {
                    assert_eq!(
                        hex::encode(tt.hll.to_bytes()),
                        hex::encode(h.to_bytes()),
                        "incorrect serialized value at line {} {test_name}",
                        line_no
                    );
                }
            }

            line_no += 1;
        }
    }
}

fn parse_add_test_case(line: &str, line_no: usize) -> AddTestCase {
    let parts: Vec<&str> = line.split(',').collect();
    assert_eq!(parts.len(), 3, "required 3 columns at line {}", line_no);

    let cardinality = parts[0]
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("invalid cardinality at line {}, {}", line_no, parts[0]));
    let to_add = parts[1]
        .parse::<i64>()
        .unwrap_or_else(|_| panic!("invalid value at line {}, {}", line_no, parts[1]));

    AddTestCase {
        hll: parse_hll(parts[2], line_no),
        to_add: to_add as u64,
        cardinality: cardinality.ceil() as u64,
    }
}

fn parse_union_test_case(line: &str, line_no: usize) -> UnionTestCase {
    let parts: Vec<&str> = line.split(',').collect();
    assert_eq!(parts.len(), 4, "required 4 columns at line {}", line_no);

    let cardinality = parts[2]
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("invalid cardinality at line {}, {}", line_no, parts[2]));

    UnionTestCase {
        hll: parse_hll(parts[3], line_no),
        to_union: parse_hll(parts[1], line_no),
        cardinality: cardinality.ceil() as u64,
    }
}

fn parse_hll(hex_encoded: &str, line_no: usize) -> Hll {
    assert!(
        hex_encoded.starts_with("\\x"),
        "missing \\x at line {}",
        line_no
    );

    let bytes = hex::decode(&hex_encoded[2..])
        .unwrap_or_else(|_| panic!("invalid hex at line {}", line_no));

    let hll = Hll::from_bytes(&bytes).unwrap_or_else(|_| panic!("invalid Hll at line {}", line_no));

    // Set sparse threshold to match Java implementation's test generator
    // This is required for tests to match up with outputs from Java library

    let mut settings = *hll.settings();
    settings.sparse_threshold = Some(850);

    hll.clone_with_settings(&settings)
}
