//! Compatibility tests for `GlyphOutline` with `oxitext-raster` path commands.
//!
//! The `oxitext-raster` crate uses a `PathCommand` type with field names
//! `x1`/`y1` for `QuadTo` control points and `x1`/`y1`/`x2`/`y2` for
//! `CubicTo`.  `oxifont_core::GlyphOutline` uses `cx`/`cy` and
//! `cx1`/`cy1`/`cx2`/`cy2` instead.  This test suite verifies that:
//!
//! 1. `GlyphOutline::transform` correctly scales and flips coordinates.
//! 2. `GlyphOutline::bounding_box` correctly accumulates control points.
//! 3. `GlyphOutline::coords` yields the expected (x, y) pairs per variant.
//! 4. A full simulated "font-to-screen-space" pipeline matches hand-computed
//!    expected values, confirming that `oxifont_backend.rs` in `oxitext-raster`
//!    can consume `GlyphOutline` slices without data loss.

use oxifont_core::GlyphOutline;

// ---------------------------------------------------------------------------
// transform tests
// ---------------------------------------------------------------------------

#[test]
fn transform_move_to_identity() {
    let cmd = GlyphOutline::MoveTo { x: 10.0, y: 20.0 };
    let out = cmd.transform(1.0, 1.0, 0.0, 0.0);
    assert_eq!(out, GlyphOutline::MoveTo { x: 10.0, y: 20.0 });
}

#[test]
fn transform_move_to_scale() {
    let cmd = GlyphOutline::MoveTo { x: 100.0, y: 200.0 };
    let scale = 0.016_f32; // 16px / 1000 units-per-em
    let out = cmd.transform(scale, -scale, 0.0, 16.0);
    match out {
        GlyphOutline::MoveTo { x, y } => {
            assert!((x - 1.6).abs() < 1e-5, "x={x}");
            // y = 200.0 * -0.016 + 16.0 = -3.2 + 16.0 = 12.8
            assert!((y - 12.8).abs() < 1e-4, "y={y}");
        }
        _ => panic!("expected MoveTo"),
    }
}

#[test]
fn transform_line_to_scale_and_offset() {
    let cmd = GlyphOutline::LineTo { x: 500.0, y: 700.0 };
    let scale = 16.0_f32 / 1000.0;
    let out = cmd.transform(scale, -scale, 0.0, 16.0);
    match out {
        GlyphOutline::LineTo { x, y } => {
            assert!((x - 8.0).abs() < 1e-5, "x={x}");
            // y = 700.0 * -0.016 + 16.0 = -11.2 + 16.0 = 4.8
            assert!((y - 4.8).abs() < 1e-4, "y={y}");
        }
        _ => panic!("expected LineTo"),
    }
}

#[test]
fn transform_quad_to_all_coords_scaled() {
    let cmd = GlyphOutline::QuadTo {
        cx: 100.0,
        cy: 200.0,
        x: 300.0,
        y: 400.0,
    };
    let s = 0.01_f32;
    let out = cmd.transform(s, -s, 0.0, 10.0);
    match out {
        GlyphOutline::QuadTo { cx, cy, x, y } => {
            assert!((cx - 1.0).abs() < 1e-5, "cx={cx}");
            assert!((cy - 8.0).abs() < 1e-5, "cy: 200*-0.01 + 10 = 8, got {cy}");
            assert!((x - 3.0).abs() < 1e-5, "x={x}");
            assert!((y - 6.0).abs() < 1e-5, "y: 400*-0.01 + 10 = 6, got {y}");
        }
        _ => panic!("expected QuadTo"),
    }
}

#[test]
fn transform_cubic_to_all_coords_scaled() {
    let cmd = GlyphOutline::CubicTo {
        cx1: 100.0,
        cy1: 200.0,
        cx2: 300.0,
        cy2: 400.0,
        x: 500.0,
        y: 600.0,
    };
    let s = 0.01_f32;
    let out = cmd.transform(s, -s, 0.0, 10.0);
    match out {
        GlyphOutline::CubicTo {
            cx1,
            cy1,
            cx2,
            cy2,
            x,
            y,
        } => {
            assert!((cx1 - 1.0).abs() < 1e-5, "cx1={cx1}");
            assert!((cy1 - 8.0).abs() < 1e-5, "cy1={cy1}");
            assert!((cx2 - 3.0).abs() < 1e-5, "cx2={cx2}");
            assert!((cy2 - 6.0).abs() < 1e-5, "cy2={cy2}");
            assert!((x - 5.0).abs() < 1e-5, "x={x}");
            assert!((y - 4.0).abs() < 1e-5, "y={y}");
        }
        _ => panic!("expected CubicTo"),
    }
}

#[test]
fn transform_close_is_noop() {
    let cmd = GlyphOutline::Close;
    let out = cmd.transform(2.0, -2.0, 5.0, 10.0);
    assert_eq!(out, GlyphOutline::Close);
}

// ---------------------------------------------------------------------------
// coords iterator tests
// ---------------------------------------------------------------------------

#[test]
fn coords_move_to_yields_one_pair() {
    let cmd = GlyphOutline::MoveTo { x: 1.0, y: 2.0 };
    let coords: Vec<(f32, f32)> = cmd.coords().collect();
    assert_eq!(coords, vec![(1.0, 2.0)]);
}

#[test]
fn coords_line_to_yields_one_pair() {
    let cmd = GlyphOutline::LineTo { x: 3.0, y: 4.0 };
    let coords: Vec<(f32, f32)> = cmd.coords().collect();
    assert_eq!(coords, vec![(3.0, 4.0)]);
}

#[test]
fn coords_quad_to_yields_two_pairs() {
    let cmd = GlyphOutline::QuadTo {
        cx: 10.0,
        cy: 20.0,
        x: 30.0,
        y: 40.0,
    };
    let coords: Vec<(f32, f32)> = cmd.coords().collect();
    assert_eq!(coords.len(), 2);
    // First pair is the control point (maps to x1/y1 in other APIs).
    assert_eq!(coords[0], (10.0, 20.0));
    // Second pair is the endpoint.
    assert_eq!(coords[1], (30.0, 40.0));
}

#[test]
fn coords_cubic_to_yields_three_pairs() {
    let cmd = GlyphOutline::CubicTo {
        cx1: 10.0,
        cy1: 20.0,
        cx2: 30.0,
        cy2: 40.0,
        x: 50.0,
        y: 60.0,
    };
    let coords: Vec<(f32, f32)> = cmd.coords().collect();
    assert_eq!(coords.len(), 3);
    assert_eq!(coords[0], (10.0, 20.0)); // cx1/cy1 → x1/y1
    assert_eq!(coords[1], (30.0, 40.0)); // cx2/cy2 → x2/y2
    assert_eq!(coords[2], (50.0, 60.0)); // endpoint
}

#[test]
fn coords_close_yields_no_pairs() {
    let cmd = GlyphOutline::Close;
    let coords: Vec<(f32, f32)> = cmd.coords().collect();
    assert!(coords.is_empty());
}

// ---------------------------------------------------------------------------
// bounding_box tests
// ---------------------------------------------------------------------------

#[test]
fn bounding_box_none_for_only_close() {
    let cmds = [GlyphOutline::Close];
    assert!(GlyphOutline::bounding_box(&cmds).is_none());
}

#[test]
fn bounding_box_empty_slice() {
    assert!(GlyphOutline::bounding_box(&[]).is_none());
}

#[test]
fn bounding_box_single_move_to() {
    let cmds = [GlyphOutline::MoveTo { x: 5.0, y: 7.0 }];
    let bbox = GlyphOutline::bounding_box(&cmds);
    assert!(bbox.is_some());
    let (x0, y0, x1, y1) = bbox.unwrap();
    assert!((x0 - 5.0).abs() < 1e-5);
    assert!((y0 - 7.0).abs() < 1e-5);
    assert!((x1 - 5.0).abs() < 1e-5);
    assert!((y1 - 7.0).abs() < 1e-5);
}

#[test]
fn bounding_box_includes_control_points() {
    // A quadratic curve whose control point bulges outside the endpoint range.
    let cmds = [
        GlyphOutline::MoveTo { x: 0.0, y: 0.0 },
        GlyphOutline::QuadTo {
            cx: 500.0, // control point extends far right
            cy: 300.0,
            x: 200.0,
            y: 100.0,
        },
        GlyphOutline::Close,
    ];
    let bbox = GlyphOutline::bounding_box(&cmds);
    assert!(bbox.is_some());
    let (x0, _y0, x1, y1) = bbox.unwrap();
    // x_max should be the control point's x (500), not the endpoint's x (200).
    assert!(
        (x1 - 500.0).abs() < 1e-5,
        "x_max should be control point: {x1}"
    );
    assert!((x0 - 0.0).abs() < 1e-5, "x_min should be 0: {x0}");
    assert!(
        (y1 - 300.0).abs() < 1e-5,
        "y_max should include control point: {y1}"
    );
}

#[test]
fn bounding_box_cubic_includes_all_control_points() {
    let cmds = [
        GlyphOutline::MoveTo { x: 0.0, y: 0.0 },
        GlyphOutline::CubicTo {
            cx1: -50.0, // control point extends left of origin
            cy1: 0.0,
            cx2: 100.0,
            cy2: 1000.0, // control point extends far up
            x: 50.0,
            y: 0.0,
        },
    ];
    let bbox = GlyphOutline::bounding_box(&cmds);
    assert!(bbox.is_some());
    let (x0, _y0, _x1, y1) = bbox.unwrap();
    assert!((x0 - -50.0).abs() < 1e-5, "x_min should be cx1=-50: {x0}");
    assert!((y1 - 1000.0).abs() < 1e-5, "y_max should be cy2=1000: {y1}");
}

// ---------------------------------------------------------------------------
// Simulated font-to-screen-space pipeline compatibility test
//
// This mirrors what oxitext-raster's OxifontRaster backend does:
//   1. Call face.outline(gid) to get Vec<GlyphOutline> (design units, Y-up)
//   2. Compute bounding box from control points
//   3. Scale + shift each command to pixel space (Y-down)
//   4. Pass each command to tiny-skia PathBuilder with x1/y1 field mapping
//
// We simulate steps 1-3 here to confirm the transforms produce the expected
// pixel coordinates, and step 4 via pattern matching.
// ---------------------------------------------------------------------------

#[test]
fn pipeline_design_to_screen_space_matches_expected() {
    // Simulate a simple triangular glyph outline in design units (1000 upm).
    let design_cmds = vec![
        GlyphOutline::MoveTo { x: 0.0, y: 0.0 },
        GlyphOutline::LineTo { x: 500.0, y: 700.0 },
        GlyphOutline::LineTo { x: 1000.0, y: 0.0 },
        GlyphOutline::Close,
    ];

    // Step 1: compute bounding box.
    let bbox = GlyphOutline::bounding_box(&design_cmds);
    assert!(bbox.is_some());
    let (min_x, min_y, max_x, max_y) = bbox.unwrap();
    assert!((min_x - 0.0).abs() < 1e-5);
    assert!((min_y - 0.0).abs() < 1e-5);
    assert!((max_x - 1000.0).abs() < 1e-5);
    assert!((max_y - 700.0).abs() < 1e-5);

    // Step 2: compute scale for 16px output (units_per_em=1000).
    let units_per_em = 1000.0_f32;
    let px_size = 16.0_f32;
    let scale = px_size / units_per_em;

    // Step 3: transform to pixel space (Y-down, origin at top-left of bbox).
    // OxifontRaster uses: tx = (x - min_x) * scale + 1.0
    //                     ty = (max_y - y) * scale + 1.0
    // This is equivalent to transform(scale, -scale, -min_x*scale+1, max_y*scale+1).
    let x_offset = -min_x * scale + 1.0;
    let y_offset = max_y * scale + 1.0;
    let pixel_cmds: Vec<GlyphOutline> = design_cmds
        .iter()
        .map(|c| c.transform(scale, -scale, x_offset, y_offset))
        .collect();

    // Verify MoveTo { x:0, y:0 } → pixel (1.0, max_y*scale+1) = (1.0, 12.2)
    match &pixel_cmds[0] {
        GlyphOutline::MoveTo { x, y } => {
            assert!((x - 1.0).abs() < 1e-4, "x={x}");
            // y = 0.0 * -scale + max_y*scale + 1.0 = 0*-0.016 + 700*0.016 + 1 = 0 + 11.2 + 1 = 12.2
            assert!((y - 12.2).abs() < 1e-4, "y={y}");
        }
        _ => panic!("expected MoveTo"),
    }

    // Verify LineTo { x:500, y:700 } → pixel (500*scale+1, 1.0) = (9.0, 1.0)
    match &pixel_cmds[1] {
        GlyphOutline::LineTo { x, y } => {
            // x = (500 - 0)*0.016 + 1 = 8.0 + 1 = 9.0
            assert!((x - 9.0).abs() < 1e-4, "x={x}");
            // y = (700 - 700)*0.016 + 1 = 0 + 1 = 1.0
            assert!((y - 1.0).abs() < 1e-4, "y={y}");
        }
        _ => panic!("expected LineTo"),
    }

    // Verify LineTo { x:1000, y:0 } → pixel (17.0, 12.2)
    match &pixel_cmds[2] {
        GlyphOutline::LineTo { x, y } => {
            // x = (1000 - 0)*0.016 + 1 = 16.0 + 1 = 17.0
            assert!((x - 17.0).abs() < 1e-4, "x={x}");
            // y = (700 - 0)*0.016 + 1 = 11.2 + 1 = 12.2
            assert!((y - 12.2).abs() < 1e-4, "y={y}");
        }
        _ => panic!("expected LineTo"),
    }

    match &pixel_cmds[3] {
        GlyphOutline::Close => {}
        _ => panic!("expected Close"),
    }
}

/// Verify that `GlyphOutline::QuadTo` field names `cx`/`cy` correspond to
/// the control point (x1/y1 in other APIs), not the endpoint.
///
/// This is the key naming-convention check ensuring downstream renderers
/// (like `oxitext-raster`) correctly map `cx → x1, cy → y1`.
#[test]
fn quad_to_cx_cy_are_control_points_not_endpoints() {
    // A quadratic Bezier: start=(0,0), control=(100,200), end=(300,0).
    // The control point is NOT on the curve.
    // At t=0.5 the curve point is: (start + 2*ctrl + end) / 4 = (0+200+300)/4=125, (0+400+0)/4=100
    let cmd = GlyphOutline::QuadTo {
        cx: 100.0, // control point (x1 in other APIs)
        cy: 200.0, // control point (y1 in other APIs)
        x: 300.0,  // endpoint
        y: 0.0,
    };
    // Confirm coords() returns control point first, then endpoint.
    let coords: Vec<(f32, f32)> = cmd.coords().collect();
    assert_eq!(
        coords[0],
        (100.0, 200.0),
        "first coord must be control point cx/cy"
    );
    assert_eq!(coords[1], (300.0, 0.0), "second coord must be endpoint x/y");
}

/// Verify `CubicTo` field naming: cx1/cy1 = first control (x1/y1),
/// cx2/cy2 = second control (x2/y2), x/y = endpoint.
#[test]
fn cubic_to_field_mapping_to_x1y1_x2y2() {
    let cmd = GlyphOutline::CubicTo {
        cx1: 10.0,
        cy1: 20.0,
        cx2: 80.0,
        cy2: 90.0,
        x: 100.0,
        y: 0.0,
    };
    let coords: Vec<(f32, f32)> = cmd.coords().collect();
    assert_eq!(
        coords[0],
        (10.0, 20.0),
        "cx1/cy1 must be first pair (maps to x1/y1)"
    );
    assert_eq!(
        coords[1],
        (80.0, 90.0),
        "cx2/cy2 must be second pair (maps to x2/y2)"
    );
    assert_eq!(coords[2], (100.0, 0.0), "x/y must be third pair (endpoint)");
}
