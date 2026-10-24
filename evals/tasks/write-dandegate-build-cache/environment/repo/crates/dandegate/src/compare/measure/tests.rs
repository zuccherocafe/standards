use super::*;

fn observed(value: f64, unit: &str, worse: Option<Direction>) -> Observation {
    Observation {
        value: Finite::new(value).expect("a finite value"),
        unit: unit.to_owned(),
        worse,
    }
}

fn single(text: &str) -> (String, Vec<Observation>) {
    let measures = Measures::from_lines(text).expect("measures");
    let (name, observations) = measures.0.into_iter().next().expect("one measure");
    (name.to_string(), observations)
}

#[test]
fn a_line_gives_a_name_a_value_a_unit_of_any_words_and_an_optional_direction() {
    assert_eq!(
        single("MEASURE: clips.walk.twitch 0.012 m higher-is-worse"),
        (
            "clips.walk.twitch".to_owned(),
            vec![observed(0.012, "m", Some(Direction::Higher))]
        )
    );
    assert_eq!(
        single("  MEASURE: a 2 mm before closure lower-is-worse").1,
        [observed(2.0, "mm before closure", Some(Direction::Lower))]
    );
    assert_eq!(
        single("MEASURE: a 15.03 deg").1,
        [observed(15.03, "deg", None)]
    );
    assert_eq!(single("MEASURE: a 3").1, [observed(3.0, "", None)]);
    assert_eq!(
        single("MEASURE: a 3 higher-is-worse").1,
        [observed(3.0, "", Some(Direction::Higher))]
    );
    // A direction word anywhere but last is part of the unit.
    assert_eq!(
        single("MEASURE: a 3 higher-is-worse m").1,
        [observed(3.0, "higher-is-worse m", None)]
    );
}

#[test]
fn other_lines_are_not_read_and_a_bad_measure_line_is_refused_with_its_number() {
    assert_eq!(
        Measures::from_lines("gate: ok\nMEASURES: a 1 m\n"),
        Ok(Measures::default())
    );
    for (text, why) in [
        ("ok\nMEASURE: a", LineProblem::Shape),
        ("ok\nMEASURE: a one m", LineProblem::Value("one".to_owned())),
        ("ok\nMEASURE: a nan m", LineProblem::Value("nan".to_owned())),
        ("ok\nMEASURE: a inf m", LineProblem::Value("inf".to_owned())),
        (
            "ok\nMEASURE: a 1e999 m",
            LineProblem::Value("1e999".to_owned()),
        ),
    ] {
        assert_eq!(
            Measures::from_lines(text),
            Err(LineError { line: 2, why }),
            "{text}"
        );
    }
}

#[test]
fn a_name_keeps_each_distinct_observation_once() {
    let (_, observations) =
        single("MEASURE: a 1 m\nMEASURE: a 1 m\nMEASURE: a 2 m\nMEASURE: a 1 mm\n");
    assert_eq!(
        observations,
        [
            observed(1.0, "m", None),
            observed(2.0, "m", None),
            observed(1.0, "mm", None)
        ]
    );
}

fn row(before: &str, after: &str) -> MeasureRow {
    let sides = PerSide {
        before: Measures::from_lines(before).expect("measures"),
        after: Measures::from_lines(after).expect("measures"),
    };
    compare(&sides).into_iter().next().expect("a row")
}

#[test]
fn each_comparison_has_its_verdict() {
    assert_eq!(
        row("MEASURE: a 1 m", "MEASURE: a 1 m").verdict,
        Verdict::Same
    );
    assert_eq!(row("", "MEASURE: a 1 m").verdict, Verdict::New);
    assert_eq!(row("MEASURE: a 1 m", "").verdict, Verdict::Gone);
    assert_eq!(
        row("MEASURE: a 1 m", "MEASURE: a 1 mm").verdict,
        Verdict::UnitChanged
    );
    assert_eq!(
        row("MEASURE: a 1 m\nMEASURE: a 2 m", "MEASURE: a 1 m").verdict,
        Verdict::Conflict
    );
    assert_eq!(
        row("MEASURE: a 4 m", "MEASURE: a 5 m").verdict,
        Verdict::Moved {
            change: Finite::new(1.0),
            relative: Finite::new(0.25),
            worse: None,
        }
    );
}

#[test]
fn a_difference_that_cannot_be_represented_is_none_and_both_values_stay() {
    // A zero before value has no relative change.
    assert_eq!(
        row("MEASURE: a 0 m", "MEASURE: a 2 m").verdict,
        Verdict::Moved {
            change: Finite::new(2.0),
            relative: None,
            worse: None,
        }
    );
    // The difference of opposite extremes overflows.
    let extreme = row(
        "MEASURE: a -1.7e308 m higher-is-worse",
        "MEASURE: a 1.7e308 m higher-is-worse",
    );
    assert_eq!(
        extreme.verdict,
        Verdict::Moved {
            change: None,
            relative: None,
            worse: Some(Direction::Higher),
        }
    );
    assert_eq!(
        extreme.sides.before,
        [observed(-1.7e308, "m", Some(Direction::Higher))]
    );
    // A tiny before value overflows the ratio.
    assert_eq!(
        row("MEASURE: a 1e-308 m", "MEASURE: a 1e308 m").verdict,
        Verdict::Moved {
            change: Finite::new(1e308),
            relative: None,
            worse: None,
        }
    );
    // A finite ratio overflows as a percentage, and the change still says it is worse.
    let percent = row(
        "MEASURE: a 1 m higher-is-worse",
        "MEASURE: a 1e308 m higher-is-worse",
    );
    assert_eq!(
        percent.verdict,
        Verdict::Moved {
            change: Finite::new(1e308),
            relative: Finite::new(1e308),
            worse: Some(Direction::Higher),
        }
    );
    assert_eq!(
        percent.change(),
        "+1e308, relative change not representable, moved, worse (higher-is-worse)"
    );
    assert_eq!(
        percent.sides.before,
        [observed(1.0, "m", Some(Direction::Higher))]
    );
}

#[test]
fn a_measure_is_worse_only_the_way_its_line_says_and_a_side_without_a_direction_borrows_the_others()
{
    let worse = |before: &str, after: &str| match row(before, after).verdict {
        Verdict::Moved { worse, .. } => worse,
        other => panic!("not moved: {other:?}"),
    };
    assert_eq!(
        worse(
            "MEASURE: a 1 m higher-is-worse",
            "MEASURE: a 2 m higher-is-worse"
        ),
        Some(Direction::Higher)
    );
    assert_eq!(
        worse(
            "MEASURE: a 2 m higher-is-worse",
            "MEASURE: a 1 m higher-is-worse"
        ),
        None
    );
    assert_eq!(
        worse(
            "MEASURE: a 2 m lower-is-worse",
            "MEASURE: a 1 m lower-is-worse"
        ),
        Some(Direction::Lower)
    );
    assert_eq!(worse("MEASURE: a 1 m", "MEASURE: a 2 m"), None);
    assert_eq!(
        worse("MEASURE: a 1 m", "MEASURE: a 2 m higher-is-worse"),
        Some(Direction::Higher)
    );
    assert_eq!(
        worse("MEASURE: a 1 m higher-is-worse", "MEASURE: a 2 m"),
        Some(Direction::Higher)
    );
    let disagree = row(
        "MEASURE: a 1 m higher-is-worse",
        "MEASURE: a 2 m lower-is-worse",
    );
    assert!(disagree.directions_disagree());
    // Either side's direction alone would call these worse. Together they disagree, so neither does.
    assert_eq!(
        worse(
            "MEASURE: a 1 m lower-is-worse",
            "MEASURE: a 2 m higher-is-worse"
        ),
        None
    );
    assert_eq!(
        worse(
            "MEASURE: a 2 m higher-is-worse",
            "MEASURE: a 1 m lower-is-worse"
        ),
        None
    );
    assert_eq!(
        worse(
            "MEASURE: a 1 m higher-is-worse",
            "MEASURE: a 2 m lower-is-worse"
        ),
        None
    );
}

#[test]
fn the_movement_marker_is_past_five_percent() {
    assert!(!row("MEASURE: a 100 m", "MEASURE: a 105 m").flagged());
    assert!(row("MEASURE: a 100 m", "MEASURE: a 105.5 m").flagged());
    assert!(row("MEASURE: a 0 m", "MEASURE: a 0.001 m").flagged());
    assert_eq!(
        row("MEASURE: a 100 m", "MEASURE: a 99 m").change(),
        "-1, 1.0%"
    );
}

#[test]
fn worse_measures_come_largest_relative_change_first_and_unrepresentable_ones_before_all() {
    let sides = PerSide {
        before: Measures::from_lines(
            "MEASURE: small 100 m higher-is-worse\nMEASURE: big 1 m higher-is-worse\nMEASURE: zero 0 m higher-is-worse\n\
             MEASURE: better 2 m higher-is-worse\n",
        )
        .expect("measures"),
        after: Measures::from_lines(
            "MEASURE: small 101 m higher-is-worse\nMEASURE: big 3 m higher-is-worse\nMEASURE: zero 1 m higher-is-worse\n\
             MEASURE: better 1 m higher-is-worse\n",
        )
        .expect("measures"),
    };
    let rows = compare(&sides);
    let names: Vec<&str> = worse(&rows).iter().map(|row| row.name().as_str()).collect();
    assert_eq!(names, ["zero", "big", "small"]);
}

#[test]
fn a_saved_record_round_trips_values_units_and_directions() {
    let measures = Measures::from_lines(
        "MEASURE: a 1.5 mm before closure lower-is-worse\nMEASURE: b 2 deg\nMEASURE: b 3 deg\n",
    )
    .expect("measures");
    let saved = measures.saved();
    let read = Measures::from_saved(&saved).expect("a record");
    assert_eq!(read, measures);
}

#[test]
fn earlier_reports_records_are_read_in_each_shape() {
    let read = Measures::from_saved(
        r#"{"a": [[1, "m"], [2.5, "m"]], "b": [3, "s"], "c": [[4, 5], "kg"]}"#,
    )
    .expect("a record");
    let expected = Measures::from_lines(
        "MEASURE: a 1 m\nMEASURE: a 2.5 m\nMEASURE: b 3 s\nMEASURE: c 4 kg\nMEASURE: c 5 kg\n",
    )
    .expect("measures");
    assert_eq!(read, expected);
    for text in [
        r#"{"a": "1 m"}"#,
        r#"{"a": [[1, 2]]}"#,
        r#"{"format": "dandegate-measures-v9", "measures": {}}"#,
        "[1]",
        "{",
    ] {
        assert!(Measures::from_saved(text).is_err(), "{text}");
    }
}

#[test]
fn own_measures_replace_imported_names_and_merged_ones_add_to_them() {
    let imported = Measures::from_lines("MEASURE: a 1 m\nMEASURE: b 1 m\n").expect("measures");
    let own = Measures::from_lines("MEASURE: a 2 m\n").expect("measures");
    let replaced = imported.clone().replaced_by(own.clone());
    assert_eq!(
        replaced.0[&MeasureName::new("a").expect("a name")],
        [observed(2.0, "m", None)]
    );
    assert_eq!(
        replaced.0[&MeasureName::new("b").expect("a name")],
        [observed(1.0, "m", None)]
    );
    let merged = imported.merged(own);
    assert_eq!(
        merged.0[&MeasureName::new("a").expect("a name")],
        [observed(1.0, "m", None), observed(2.0, "m", None)]
    );
}

#[test]
fn a_number_is_written_shortest_with_an_exponent_only_at_the_extremes() {
    for (value, text) in [
        (0.0, "0"),
        (0.012, "0.012"),
        (15.03, "15.03"),
        (1e15, "1e15"),
        (-1.7e308, "-1.7e308"),
        (2e-6, "2e-6"),
    ] {
        assert_eq!(
            Finite::new(value).expect("finite").to_string(),
            text,
            "{value}"
        );
    }
}
