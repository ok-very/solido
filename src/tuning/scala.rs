use std::fmt;

/// A single scale degree — either an interval in cents or a frequency ratio.
#[derive(Debug, Clone)]
pub enum ScalaDegree {
    Cents(f64),
    Ratio(u64, u64),
}

impl ScalaDegree {
    /// Convert this degree to cents. For ratios: 1200 * log2(num/den).
    pub fn to_cents(&self) -> f64 {
        match self {
            ScalaDegree::Cents(c) => *c,
            ScalaDegree::Ratio(num, den) => 1200.0 * (*num as f64 / *den as f64).log2(),
        }
    }
}

/// Error type for Scala (.scl) parsing failures.
#[derive(Debug)]
pub enum ScalaParseError {
    Empty,
    InvalidDegreeCount(String),
    InvalidDegree { line: usize, text: String },
    NotEnoughDegrees { expected: usize, found: usize },
    ZeroDenominator { line: usize },
}

impl fmt::Display for ScalaParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScalaParseError::Empty => write!(f, "empty .scl file"),
            ScalaParseError::InvalidDegreeCount(s) => write!(f, "invalid degree count: '{s}'"),
            ScalaParseError::InvalidDegree { line, text } => {
                write!(f, "invalid degree at line {line}: '{text}'")
            }
            ScalaParseError::NotEnoughDegrees { expected, found } => {
                write!(f, "expected {expected} degrees, found {found}")
            }
            ScalaParseError::ZeroDenominator { line } => {
                write!(f, "zero denominator in ratio at line {line}")
            }
        }
    }
}

impl std::error::Error for ScalaParseError {}

/// A parsed tuning system from a .scl file.
///
/// `degrees` stores the raw ScalaDegree values (excludes root, includes period).
/// `cents` is a sorted cache that INCLUDES 0.0 at the root for easy nearest-degree search.
#[derive(Debug, Clone)]
pub struct TuningSystem {
    pub name: String,
    pub description: String,
    pub degrees: Vec<ScalaDegree>,
    /// Cached cents values, sorted ascending. Index 0 = root (0.0).
    /// Length = degrees.len() + 1.
    pub cents: Vec<f64>,
}

impl TuningSystem {
    /// Parse a Scala (.scl) format string into a TuningSystem.
    ///
    /// Format:
    /// - Lines starting with `!` are comments
    /// - First non-comment line: description
    /// - Second non-comment line: degree count (integer)
    /// - Remaining: one degree each (decimal = cents, fraction = ratio, bare int = ratio N/1)
    /// - Inline `!` on degree lines strips trailing comments
    pub fn from_scl(source: &str) -> Result<Self, ScalaParseError> {
        let mut non_comment: Vec<(usize, &str)> = Vec::new();

        for (i, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('!') {
                continue;
            }
            non_comment.push((i + 1, trimmed));
        }

        if non_comment.len() < 2 {
            return Err(ScalaParseError::Empty);
        }

        let description = non_comment[0].1.to_string();

        let count_str = non_comment[1].1;
        let degree_count: usize = count_str
            .parse()
            .map_err(|_| ScalaParseError::InvalidDegreeCount(count_str.to_string()))?;

        let mut degrees = Vec::with_capacity(degree_count);

        for i in 0..degree_count {
            let idx = i + 2;
            if idx >= non_comment.len() {
                return Err(ScalaParseError::NotEnoughDegrees {
                    expected: degree_count,
                    found: i,
                });
            }

            let (line_num, raw) = non_comment[idx];
            // Strip inline comments after `!`
            let text = match raw.find('!') {
                Some(pos) => raw[..pos].trim(),
                None => raw.trim(),
            };

            degrees.push(Self::parse_degree(line_num, text)?);
        }

        let mut cents = Vec::with_capacity(degrees.len() + 1);
        cents.push(0.0);
        for d in &degrees {
            cents.push(d.to_cents());
        }

        Ok(TuningSystem {
            name: String::new(),
            description,
            degrees,
            cents,
        })
    }

    fn parse_degree(line_num: usize, text: &str) -> Result<ScalaDegree, ScalaParseError> {
        if text.contains('/') {
            let parts: Vec<&str> = text.splitn(2, '/').collect();
            let num: u64 =
                parts[0]
                    .trim()
                    .parse()
                    .map_err(|_| ScalaParseError::InvalidDegree {
                        line: line_num,
                        text: text.to_string(),
                    })?;
            let den: u64 =
                parts[1]
                    .trim()
                    .parse()
                    .map_err(|_| ScalaParseError::InvalidDegree {
                        line: line_num,
                        text: text.to_string(),
                    })?;
            if den == 0 {
                return Err(ScalaParseError::ZeroDenominator { line: line_num });
            }
            return Ok(ScalaDegree::Ratio(num, den));
        }

        if text.contains('.') {
            let val: f64 =
                text.parse()
                    .map_err(|_| ScalaParseError::InvalidDegree {
                        line: line_num,
                        text: text.to_string(),
                    })?;
            return Ok(ScalaDegree::Cents(val));
        }

        // Bare integer → ratio N/1
        let num: u64 =
            text.parse()
                .map_err(|_| ScalaParseError::InvalidDegree {
                    line: line_num,
                    text: text.to_string(),
                })?;
        Ok(ScalaDegree::Ratio(num, 1))
    }

    /// The period of this tuning in cents (last degree's cents value).
    /// Typically 1200.0 for octave-repeating, ~1901.96 for Bohlen-Pierce.
    pub fn period_cents(&self) -> f64 {
        *self.cents.last().unwrap_or(&1200.0)
    }

    /// Convert a scale degree + octave offset to Hz.
    ///
    /// - `degree`: index into `self.cents` (0 = root)
    /// - `octave`: number of periods above/below root
    /// - `root_hz`: frequency of the root note (e.g. 261.63 for C4)
    pub fn degree_to_hz(&self, degree: usize, octave: i32, root_hz: f64) -> f64 {
        let degree_cents = self.cents.get(degree).copied().unwrap_or(0.0);
        let total_cents = degree_cents + (octave as f64 * self.period_cents());
        root_hz * 2.0_f64.powf(total_cents / 1200.0)
    }

    /// Find the nearest scale degree to a given cents value.
    ///
    /// Input is folded into one period via rem_euclid.
    /// Returns `(degree_index, signed_distance_in_cents)`.
    pub fn nearest_degree(&self, cents: f64) -> (usize, f64) {
        let period = self.period_cents();
        let folded = cents.rem_euclid(period);

        let mut best_idx = 0;
        let mut best_dist = folded; // distance from root (0.0)

        // Skip the last entry (period endpoint) — it's the same pitch as root
        // and is handled by the wrap-around check below.
        let inner_count = if self.cents.len() > 1 {
            self.cents.len() - 1
        } else {
            self.cents.len()
        };

        for (i, &deg_cents) in self.cents[..inner_count].iter().enumerate() {
            let dist = folded - deg_cents;
            if dist.abs() < best_dist.abs() {
                best_idx = i;
                best_dist = dist;
            }
        }

        // Check wrap-around: distance to root from above
        let wrap_dist = folded - period;
        if wrap_dist.abs() < best_dist.abs() {
            best_idx = 0;
            best_dist = wrap_dist;
        }

        (best_idx, best_dist)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BHAIRAV_SCL: &str = "\
! bhairav.scl
!
Bhairav raga
7
!
112.00000
386.31371 ! 5/4
498.04500 ! 4/3
701.95500 ! 3/2
813.68629 ! 8/5
1088.26871 ! 15/8
2/1
";

    #[test]
    fn parse_bhairav_degree_count() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        assert_eq!(ts.degrees.len(), 7);
        assert_eq!(ts.cents.len(), 8); // root + 7
    }

    #[test]
    fn parse_bhairav_cents_values() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        assert!((ts.cents[0] - 0.0).abs() < 0.01);
        assert!((ts.cents[1] - 112.0).abs() < 0.01);
        assert!((ts.cents[2] - 386.31).abs() < 0.01);
        assert!((ts.cents[3] - 498.04).abs() < 0.01);
        assert!((ts.cents[4] - 701.96).abs() < 0.01);
        assert!((ts.cents[5] - 813.69).abs() < 0.01);
        assert!((ts.cents[6] - 1088.27).abs() < 0.01);
        assert!((ts.cents[7] - 1200.0).abs() < 0.01);
    }

    #[test]
    fn parse_ratio_5_4_to_cents() {
        let d = ScalaDegree::Ratio(5, 4);
        let cents = d.to_cents();
        assert!((cents - 386.31).abs() < 0.01, "5/4 should be ~386.31, got {cents}");
    }

    #[test]
    fn parse_ratio_2_1_is_1200_cents() {
        let d = ScalaDegree::Ratio(2, 1);
        assert!((d.to_cents() - 1200.0).abs() < 0.001);
    }

    #[test]
    fn degree_to_hz_bhairav_pa() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        let hz = ts.degree_to_hz(4, 0, 261.63);
        assert!((hz - 392.44).abs() < 0.5, "Pa should be ~392Hz, got {hz}");
    }

    #[test]
    fn degree_to_hz_bhairav_all() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        let expected = [261.63, 279.07, 327.03, 348.83, 392.44, 418.60, 490.55, 523.25];
        for (i, &expect) in expected.iter().enumerate() {
            let hz = ts.degree_to_hz(i, 0, 261.63);
            assert!((hz - expect).abs() < 0.5, "degree {i}: expected {expect}, got {hz}");
        }
    }

    #[test]
    fn degree_to_hz_octave_shift() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        let sa_0 = ts.degree_to_hz(0, 0, 261.63);
        let sa_1 = ts.degree_to_hz(0, 1, 261.63);
        assert!((sa_1 / sa_0 - 2.0).abs() < 0.01, "octave should double frequency");
    }

    #[test]
    fn nearest_degree_exact_match() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        let (idx, dist) = ts.nearest_degree(701.955);
        assert_eq!(idx, 4, "701.955 cents should be Pa (index 4)");
        assert!(dist.abs() < 0.01);
    }

    #[test]
    fn nearest_degree_between_degrees() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        // 550 cents is between Ma (498) and Pa (702) — closer to Ma
        let (idx, _) = ts.nearest_degree(550.0);
        assert_eq!(idx, 3, "550 cents should snap to Ma (498, idx 3)");
    }

    #[test]
    fn nearest_degree_wrap_around() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        // 1150 cents: Ni=1088 (dist=62) vs Sa'=1200 (dist=50) — closer to Sa'
        let (idx, _) = ts.nearest_degree(1150.0);
        assert_eq!(idx, 0, "1150 cents should wrap to Sa (0)");
    }

    #[test]
    fn period_cents_octave() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        assert!((ts.period_cents() - 1200.0).abs() < 0.01);
    }

    #[test]
    fn parse_empty_returns_error() {
        let result = TuningSystem::from_scl("! only comments\n! nothing else\n");
        assert!(result.is_err());
    }

    #[test]
    fn parse_zero_denominator_returns_error() {
        let result = TuningSystem::from_scl("test\n1\n5/0\n");
        assert!(matches!(result, Err(ScalaParseError::ZeroDenominator { .. })));
    }

    #[test]
    fn parse_not_enough_degrees() {
        let result = TuningSystem::from_scl("test\n3\n100.0\n200.0\n");
        assert!(matches!(
            result,
            Err(ScalaParseError::NotEnoughDegrees { .. })
        ));
    }

    #[test]
    fn parse_description_preserved() {
        let ts = TuningSystem::from_scl(BHAIRAV_SCL).unwrap();
        assert_eq!(ts.description, "Bhairav raga");
    }

    #[test]
    fn parse_bare_integer_as_ratio() {
        let result = TuningSystem::from_scl("test\n1\n3\n");
        let ts = result.unwrap();
        // 3 = ratio 3/1 = 1200 * log2(3) ≈ 1901.96
        assert!((ts.cents[1] - 1901.96).abs() < 0.01);
    }
}
