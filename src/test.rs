#![cfg(test)]

use crate::facts::{AllFacts, Loan, Point, Region, Variable};
use crate::intern;
use crate::program::parse_from_program;
use crate::tab_delim;
use crate::test_util::assert_equal;
use failure::Error;
use polonius_engine::{Algorithm, Output};
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn test_facts(all_facts: &AllFacts, algorithms: &[Algorithm]) {
    let naive = Output::compute(all_facts, Algorithm::Naive, true);

    // Check that the "naive errors" are a subset of the "insensitive
    // ones".
    let insensitive = Output::compute(all_facts, Algorithm::LocationInsensitive, false);
    for (naive_point, naive_loans) in &naive.errors {
        match insensitive.errors.get(&naive_point) {
            Some(insensitive_loans) => {
                for naive_loan in naive_loans {
                    if !insensitive_loans.contains(naive_loan) {
                        panic!(
                            "naive analysis had error for `{:?}` at `{:?}` \
                             but insensitive analysis did not \
                             (loans = {:#?})",
                            naive_loan, naive_point, insensitive_loans,
                        );
                    }
                }
            }

            None => {
                panic!(
                    "naive analysis had errors at `{:?}` but insensitive analysis did not \
                     (loans = {:#?})",
                    naive_point, naive_loans,
                );
            }
        }
    }

    // The optimized checks should behave exactly the same as the naive check.
    for &optimized_algorithm in algorithms {
        println!("Algorithm {:?}", optimized_algorithm);
        let opt = Output::compute(all_facts, optimized_algorithm, true);
        assert_equal(&naive.borrow_live_at, &opt.borrow_live_at);
        assert_equal(&naive.errors, &opt.errors);
    }

    // The hybrid algorithm gets the same errors as the naive version
    let opt = Output::compute(all_facts, Algorithm::Hybrid, true);
    assert_equal(&naive.errors, &opt.errors);
}

fn test_fn(dir_name: &str, fn_name: &str, algorithm: Algorithm) -> Result<(), Error> {
    let facts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("inputs")
        .join(dir_name)
        .join("nll-facts")
        .join(fn_name);
    println!("facts_dir = {:?}", facts_dir);
    let tables = &mut intern::InternerTables::new();
    let all_facts = tab_delim::load_tab_delimited_facts(tables, &facts_dir)?;
    Ok(test_facts(&all_facts, &[algorithm]))
}

macro_rules! tests {
    ($($name:ident($dir:expr, $fn:expr),)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn datafrog_opt() -> Result<(), Error> {
                    test_fn($dir, $fn, Algorithm::DatafrogOpt)
                }
            }
        )*
    }
}

tests! {
    issue_47680("issue-47680", "main"),
    vec_push_ref_foo1("vec-push-ref", "foo1"),
    vec_push_ref_foo2("vec-push-ref", "foo2"),
    vec_push_ref_foo3("vec-push-ref", "foo3"),
}

#[test]
fn test_insensitive_errors() -> Result<(), Error> {
    let facts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("inputs")
        .join("issue-47680")
        .join("nll-facts")
        .join("main");
    println!("facts_dir = {:?}", facts_dir);
    let tables = &mut intern::InternerTables::new();
    let all_facts = tab_delim::load_tab_delimited_facts(tables, &facts_dir)?;
    let insensitive = Output::compute(&all_facts, Algorithm::LocationInsensitive, false);

    let mut expected = FxHashMap::default();
    expected.insert(Point::from(22), vec![Loan::from(1)]);
    expected.insert(Point::from(46), vec![Loan::from(2)]);

    assert_equal(&insensitive.errors, &expected);
    Ok(())
}

#[test]
fn test_sensitive_passes_issue_47680() -> Result<(), Error> {
    let facts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("inputs")
        .join("issue-47680")
        .join("nll-facts")
        .join("main");
    let tables = &mut intern::InternerTables::new();
    let all_facts = tab_delim::load_tab_delimited_facts(tables, &facts_dir)?;
    let sensitive = Output::compute(&all_facts, Algorithm::DatafrogOpt, false);

    assert!(sensitive.errors.is_empty());
    Ok(())
}

#[test]
fn no_subset_symmetries_exist() -> Result<(), Error> {
    let facts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("inputs")
        .join("issue-47680")
        .join("nll-facts")
        .join("main");
    let tables = &mut intern::InternerTables::new();
    let all_facts = tab_delim::load_tab_delimited_facts(tables, &facts_dir)?;

    let subset_symmetries_exist = |output: &Output<Region, Loan, Point, Variable>| {
        for (_, subsets) in &output.subset {
            for (r1, rs) in subsets {
                if rs.contains(&r1) {
                    return true;
                }
            }
        }
        false
    };

    let naive = Output::compute(&all_facts, Algorithm::Naive, true);
    assert!(!subset_symmetries_exist(&naive));

    // FIXME: the issue-47680 dataset is suboptimal here as DatafrogOpt does not
    // produce subset symmetries for it. It does for clap, and it was used to manually verify
    // that the assert in verbose  mode didn't trigger. Therefore, switch to this dataset
    // whenever it's fast enough to be enabled in tests, or somehow create a test facts program
    // or reduce it from clap.
    let opt = Output::compute(&all_facts, Algorithm::DatafrogOpt, true);
    assert!(!subset_symmetries_exist(&opt));
    Ok(())
}

// The following 3 tests, `send_is_not_static_std_sync`, `escape_upvar_nested`, and `issue_31567`
// are extracted from rustc's test suite, and fail because of differences between the Naive
// and DatafrogOpt variants, on the computation of the transitive closure.
// They are part of the same pattern that the optimized variant misses, and only differ in
// the length of the `outlives` chain reaching a live region at a specific point.

#[test]
fn send_is_not_static_std_sync() {
    // Reduced from rustc test: ui/span/send-is-not-static-std-sync.rs
    // (in the functions: `mutex` and `rwlock`)
    let program = r"
        universal_regions { }
        block B0 {
            borrow_region_at('a, L0), outlives('a: 'b), region_live_at('b);
        }
    ";

    let mut tables = intern::InternerTables::new();
    let facts = parse_from_program(program, &mut tables).expect("Parsing failure");
    test_facts(&facts, Algorithm::OPTIMIZED);
}

#[test]
fn escape_upvar_nested() {
    // Reduced from rustc test: ui/nll/closure-requirements/escape-upvar-nested.rs
    // (in the function: `test-\{\{closure\}\}-\{\{closure\}\}/`)
    // This reduction is also present in other tests:
    // - ui/nll/closure-requirements/escape-upvar-ref.rs, in the `test-\{\{closure\}\}/` function
    let program = r"
        universal_regions { }
        block B0 {
            borrow_region_at('a, L0), outlives('a: 'b), outlives('b: 'c), region_live_at('c);
        }
    ";

    let mut tables = intern::InternerTables::new();
    let facts = parse_from_program(program, &mut tables).expect("Parsing failure");
    test_facts(&facts, Algorithm::OPTIMIZED);
}

#[test]
fn issue_31567() {
    // Reduced from rustc test: ui/nll/issue-31567.rs
    // This is one of two tuples present in the Naive results and missing from the Opt results,
    // the second tuple having the same pattern as the one in this test.
    // This reduction is also present in other tests:
    // - ui/issue-48803.rs, in the `flatten` function
    let program = r"
        universal_regions { }
        block B0 {
            borrow_region_at('a, L0),
            outlives('a: 'b),
            outlives('b: 'c),
            outlives('c: 'd),
            region_live_at('d);
        }
    ";

    let mut tables = intern::InternerTables::new();
    let facts = parse_from_program(program, &mut tables).expect("Parsing failure");
    test_facts(&facts, Algorithm::OPTIMIZED);
}

#[test]
fn borrowed_local_error() {
    // This test is related to the previous 3: there is still a borrow_region outliving a live region,
    // through a chain of `outlives` at a single point, but this time there are also 2 points
    // and an edge.

    // Reduced from rustc test: ui/nll/borrowed-local-error.rs
    // (in the function: `gimme`)
    // This reduction is also present in other tests:
    // - ui/nll/borrowed-temporary-error.rs, in the `gimme` function
    // - ui/nll/borrowed-referent-issue-38899.rs, in the `bump` function
    // - ui/nll/return-ref-mut-issue-46557.rs, in the `gimme_static_mut` function
    // - ui/span/dropck_direct_cycle_with_drop.rs, in the `{{impl}}[1]-drop-{{closure}}` function
    // - ui/span/wf-method-late-bound-regions.rs, in the `{{impl}}-xmute` function
    let program = r"
        universal_regions { 'c }
        block B0 {
            borrow_region_at('a, L0), outlives('a: 'b), outlives('b: 'c);
        }
    ";

    let mut tables = intern::InternerTables::new();
    let facts = parse_from_program(program, &mut tables).expect("Parsing failure");
    test_facts(&facts, Algorithm::OPTIMIZED);
}

#[test]
fn smoke_test_errors() {
    let failures = [
        "return_ref_to_local",
        "use_while_mut",
        "use_while_mut_fr",
        "well_formed_function_inputs",
    ];

    for test_fn in &failures {
        let facts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("inputs")
            .join("smoke-test")
            .join("nll-facts")
            .join(test_fn);
        println!("facts_dir = {:?}", facts_dir);
        let tables = &mut intern::InternerTables::new();
        let facts = tab_delim::load_tab_delimited_facts(tables, &facts_dir).expect("facts");

        let location_insensitive = Output::compute(&facts, Algorithm::LocationInsensitive, true);
        assert!(
            !location_insensitive.errors.is_empty(),
            format!("LocationInsensitive didn't find errors for '{}'", test_fn)
        );

        let naive = Output::compute(&facts, Algorithm::Naive, true);
        assert!(
            !naive.errors.is_empty(),
            format!("Naive didn't find errors for '{}'", test_fn)
        );

        let opt = Output::compute(&facts, Algorithm::DatafrogOpt, true);
        assert!(
            !opt.errors.is_empty(),
            format!("DatafrogOpt didn't find errors for '{}'", test_fn)
        );
    }
}

#[test]
fn smoke_test_success_1() {
    let facts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("inputs")
        .join("smoke-test")
        .join("nll-facts")
        .join("position_dependent_outlives");
    println!("facts_dir = {:?}", facts_dir);
    let tables = &mut intern::InternerTables::new();
    let facts = tab_delim::load_tab_delimited_facts(tables, &facts_dir).expect("facts");

    let location_insensitive = Output::compute(&facts, Algorithm::LocationInsensitive, true);
    assert!(!location_insensitive.errors.is_empty());

    test_facts(&facts, Algorithm::OPTIMIZED);
}

#[test]
fn smoke_test_success_2() {
    let facts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("inputs")
        .join("smoke-test")
        .join("nll-facts")
        .join("foo");
    println!("facts_dir = {:?}", facts_dir);
    let tables = &mut intern::InternerTables::new();
    let facts = tab_delim::load_tab_delimited_facts(tables, &facts_dir).expect("facts");

    let location_insensitive = Output::compute(&facts, Algorithm::LocationInsensitive, true);
    assert!(location_insensitive.errors.is_empty());

    test_facts(&facts, Algorithm::OPTIMIZED);
}

#[test]
// V used in P => V live upon entry into P
fn var_live_in_single_block() {
    let program = r"
        universal_regions {  }

        block B0 {
            var_used(V1);
            goto B1;
        }
    ";

    let mut tables = intern::InternerTables::new();
    let facts = parse_from_program(program, &mut tables).expect("Parsing failure");

    let liveness = Output::compute(&facts, Algorithm::Naive, true).var_live_at;
    println!("Registered liveness data: {:?}", liveness);
    for (point, variables) in liveness.iter() {
        println!("{:?} has live variables: {:?}", point, variables);
        assert_eq!(variables.len(), 1);
    }
    assert_eq!(liveness.len(), 2);
}

#[test]
// P GOTO Q, V used in Q => V live in P
fn var_live_in_successor_propagates_to_predecessor() {
    let program = r"
        universal_regions {  }

        block B0 {
            invalidates(L0); // generate a point
            goto B1;
        }

        block B1 {
            invalidates(L0);
            goto B2;
        }

        block B2 {
            invalidates(L0);
            var_used(V1);
        }
    ";

    let mut tables = intern::InternerTables::new();
    let facts = parse_from_program(program, &mut tables).expect("Parsing failure");

    let liveness = Output::compute(&facts, Algorithm::Naive, true).var_live_at;
    println!("Registered liveness data: {:?}", liveness);
    println!("CFG: {:?}", facts.cfg_edge);
    for (point, variables) in liveness.iter() {
        println!("{:?} has live variables: {:?}", point, variables);
        assert_eq!(variables.len(), 1);
    }

    assert!(!liveness.get(&0.into()).unwrap().is_empty());
}

#[test]
// P GOTO Q, V used in Q, V defined in P => V not live in P
fn var_live_in_successor_killed_by_reassignment() {
    let program = r"
        universal_regions {  }

        block B0 {
            invalidates(L0); // generate a point
            goto B1;
        }

        block B1 {
            var_defined(V1); // V1 dies
            invalidates(L0);
            goto B2;
        }

        block B2 {
            invalidates(L0);
            var_used(V1);
        }
    ";

    let mut tables = intern::InternerTables::new();
    let facts = parse_from_program(program, &mut tables).expect("Parsing failure");

    let result = Output::compute(&facts, Algorithm::Naive, true);
    println!("result: {:#?}", result);
    let liveness = result.var_live_at;
    println!("CFG: {:#?}", facts.cfg_edge);

    let first_defined: Point = 3.into(); // Mid(B1[0])

    for (&point, variables) in liveness.iter() {
        println!(
            "{} ({:?}) has live variables: {:?}",
            tables.points.untern(point),
            point,
            tables.variables.untern_vec(variables)
        );
    }

    let live_at_start = liveness.get(&0.into());

    assert_eq!(
        liveness.get(&0.into()),
        None,
        "{:?} were live at start!",
        live_at_start.and_then(|var| Some(tables.variables.untern_vec(var))),
    );

    let live_at_defined = liveness.get(&first_defined);

    assert_eq!(
        live_at_defined,
        None,
        "{:?} were alive at {}",
        live_at_defined.and_then(|var| Some(tables.variables.untern_vec(var))),
        tables.points.untern(first_defined)
    );
}

#[test]
fn var_drop_used_simple() {
    let program = r"
        universal_regions {  }

        block B0 {
            invalidates(L0); // generate a point
            goto B1;
        }

        block B1 {
            var_defined(V1); // V1 dies
            invalidates(L0);
            goto B2;
        }

        block B2 {
            invalidates(L0);
            var_drop_used(V1);
        }
    ";

    let mut tables = intern::InternerTables::new();
    let facts = parse_from_program(program, &mut tables).expect("Parsing failure");

    let result = Output::compute(&facts, Algorithm::Naive, true);
    println!("result: {:#?}", result);
    let liveness = result.var_drop_live_at;
    println!("CFG: {:#?}", facts.cfg_edge);
    let first_defined: Point = 3.into(); // Mid(B1[0])

    for (&point, variables) in liveness.iter() {
        println!(
            "{} ({:?}) has live variables: {:?}",
            tables.points.untern(point),
            point,
            tables.variables.untern_vec(variables)
        );
    }

    let live_at_start = liveness.get(&0.into());

    assert_eq!(
        liveness.get(&0.into()),
        None,
        "{:?} were live at start!",
        live_at_start.and_then(|var| Some(tables.variables.untern_vec(var))),
    );

    let live_at_defined = liveness.get(&first_defined);

    assert_eq!(
        live_at_defined,
        None,
        "{:?} were alive at {}",
        live_at_defined.and_then(|var| Some(tables.variables.untern_vec(var))),
        tables.points.untern(first_defined)
    );
}

fn untern_region_live_at(
    region_live_at: FxHashMap<Point, Vec<Region>>,
    tables: &intern::InternerTables,
) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut result = BTreeMap::default();

    for (location, regions) in &region_live_at {
        let region_names: BTreeSet<&str> = tables.regions.untern_vec(regions).into_iter().collect();
        let location_name = tables.points.untern(*location);
        result.insert(location_name, region_names);
    }

    result
}

fn compare_region_live_at(dir_name: &str, fn_name: &str) {
    let facts_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("inputs")
        .join(dir_name)
        .join("nll-facts")
        .join(fn_name);

    let mut input_region_live_at = FxHashMap::default();
    let tables = &mut intern::InternerTables::new();
    let mut all_facts = tab_delim::load_tab_delimited_facts(tables, &facts_dir).unwrap();
    for (region, location) in &all_facts.region_live_at {
        input_region_live_at
            .entry(*location)
            .or_insert_with(Vec::new)
            .push(*region);
    }
    all_facts.region_live_at = Vec::default();

    let all_points: BTreeSet<Point> = all_facts
        .cfg_edge
        .iter()
        .map(|&(p, _)| p)
        .chain(all_facts.cfg_edge.iter().map(|&(_, q)| q))
        .collect();

    for &region in &all_facts.universal_region {
        for &location in &all_points {
            input_region_live_at
                .entry(location)
                .or_insert_with(Vec::new)
                .push(region);
        }
    }

    let output_region_live_at = untern_region_live_at(
        Output::compute(&all_facts, Algorithm::Naive, true)
            .region_live_at
            .into(),
        &tables,
    );

    let input_region_live_at = untern_region_live_at(input_region_live_at, tables);

    assert_equal(&input_region_live_at, &output_region_live_at);
}

macro_rules! region_live_at_tests {
    ($($name:ident($dir:expr, $fn:expr),)*) => {
        $(
            mod $name {
                use super::*;

                #[test]
                fn computed_region_live_at_same_as_input() {
                    compare_region_live_at($dir, $fn)
                }
            }
        )*
    }
}

region_live_at_tests! {
    drop_may_dangle_main("drop-may-dangle", "main"),
    drop_liveness_main("drop-liveness", "main"),
    enum_drop_access_drop_enum("enum-drop-access", "drop_enum"),
    enum_drop_access_optional_drop_enum("enum-drop-access", "optional_drop_enum"),
    issue_52059_report_when_borrow_and_drop_conflict_1("issue-52059-report-when-borrow-and-drop-conflict", "finish_1"),
    issue_52059_report_when_borrow_and_drop_conflict_2("issue-52059-report-when-borrow-and-drop-conflict", "finish_2"),
    issue_52059_report_when_borrow_and_drop_conflict_4("issue-52059-report-when-borrow-and-drop-conflict", "finish_4"),
    issue_52059_report_when_borrow_and_drop_conflict_3("issue-52059-report-when-borrow-and-drop-conflict", "finish_3"),
    maybe_initialized_drop_main("maybe-initialized-drop", "main"),
    maybe_initialized_drop_implicit_fragment_drop_main("maybe-initialized-drop-implicit-fragment-drop", "main"),
    maybe_initialized_drop_with_fragment_main("maybe-initialized-drop-with-fragment", "main"),
    maybe_initialized_drop_with_uninitialized_fragments_main("maybe-initialized-drop-with-uninitialized-fragments", "main"),
}
