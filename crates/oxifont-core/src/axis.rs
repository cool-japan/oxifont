//! Variable-font axis types (`fvar` table records).

extern crate alloc;

/// A single OpenType/TrueType variable-font axis record from the `fvar` table.
///
/// # Example
/// ```
/// use oxifont_core::VariationAxis;
/// let axis = VariationAxis {
///     tag: *b"wght",
///     min_value: 100.0,
///     default_value: 400.0,
///     max_value: 900.0,
///     name: "Weight".to_string(),
/// };
/// assert_eq!(&axis.tag, b"wght");
/// assert_eq!(axis.default_value, 400.0);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VariationAxis {
    /// Four-byte axis tag, e.g. `b"wght"`.
    pub tag: [u8; 4],
    /// Minimum value for the axis.
    pub min_value: f32,
    /// Default (initial) value for the axis.
    pub default_value: f32,
    /// Maximum value for the axis.
    pub max_value: f32,
    /// Human-readable axis name, resolved from the `name` table (preferring an
    /// English Unicode record). Falls back to the numeric name ID rendered as a
    /// string only when the `name` table has no matching record.
    pub name: alloc::string::String,
}
