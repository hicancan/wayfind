use crate::{Error, Position, Result};

pub(crate) const R: f64 = 6_371_008.8;

pub(crate) fn validate_position(p: Position) -> Result<()> {
    if p[0].is_finite() && p[1].is_finite() && p[0].abs() <= 180.0 && p[1].abs() <= 90.0 {
        Ok(())
    } else {
        Err(Error::Invalid(
            "position must be finite WGS84 longitude/latitude".into(),
        ))
    }
}

pub fn haversine(a: Position, b: Position) -> f64 {
    let lat = ((b[1] - a[1]).to_radians() / 2.0).sin();
    let lon = ((b[0] - a[0]).to_radians() / 2.0).sin();
    let h = lat * lat + a[1].to_radians().cos() * b[1].to_radians().cos() * lon * lon;
    2.0 * R * h.clamp(0.0, 1.0).sqrt().asin()
}

pub fn bearing(a: Position, b: Position) -> f64 {
    let d = (b[0] - a[0]).to_radians();
    let (la, lb) = (a[1].to_radians(), b[1].to_radians());
    (d.sin() * lb.cos())
        .atan2(la.cos() * lb.sin() - la.sin() * lb.cos() * d.cos())
        .to_degrees()
        .rem_euclid(360.0)
}

pub(crate) fn angle(before: f64, after: f64) -> f64 {
    (after - before + 180.0).rem_euclid(360.0) - 180.0
}

pub(crate) fn xyz(p: Position) -> [f64; 3] {
    let (lon, lat) = (p[0].to_radians(), p[1].to_radians());
    [
        R * lat.cos() * lon.cos(),
        R * lat.cos() * lon.sin(),
        R * lat.sin(),
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let n = dot(a, a).sqrt();
    a.map(|x| x / n)
}
fn position(v: [f64; 3]) -> Position {
    [
        v[1].atan2(v[0]).to_degrees(),
        v[2].atan2(v[0].hypot(v[1])).to_degrees(),
    ]
}

/// Interpolate on the minor great-circle arc, including across the date line.
pub(crate) fn interpolate(a: Position, b: Position, t: f64) -> Position {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    let d = haversine(a, b) / R;
    if d < 1e-12 {
        return a;
    }
    let (av, bv) = (xyz(a), xyz(b));
    let u = ((1.0 - t) * d).sin() / d.sin();
    let v = (t * d).sin() / d.sin();
    position(std::array::from_fn(|i| av[i] * u + bv[i] * v))
}

/// Closest point on the minor great-circle arc; fraction is geometric length.
pub(crate) fn project(p: Position, a: Position, b: Position) -> (Position, f64, f64) {
    let length = haversine(a, b);
    if length < 1e-6 {
        return (a, 0.0, haversine(p, a));
    }
    let (av, bv, pv) = (unit(xyz(a)), unit(xyz(b)), unit(xyz(p)));
    let n = cross(av, bv);
    let nn = dot(n, n);
    if nn > 1e-20 {
        let pn = dot(pv, n) / nn;
        let qv = std::array::from_fn(|i| pv[i] - n[i] * pn);
        if dot(qv, qv) > 1e-20 {
            let q = position(qv);
            let aq = haversine(a, q);
            if aq + haversine(q, b) <= length + 1e-4 {
                return (q, (aq / length).clamp(0.0, 1.0), haversine(p, q));
            }
        }
    }
    let (da, db) = (haversine(p, a), haversine(p, b));
    if da <= db {
        (a, 0.0, da)
    } else {
        (b, 1.0, db)
    }
}

pub(crate) fn cumulative(points: &[Position]) -> Vec<f64> {
    let mut out = vec![0.0];
    for pair in points.windows(2) {
        out.push(out.last().copied().unwrap_or(0.0) + haversine(pair[0], pair[1]));
    }
    out
}

pub(crate) fn point_at(points: &[Position], offsets: &[f64], offset: f64) -> Position {
    let index = offsets
        .partition_point(|x| *x <= offset)
        .saturating_sub(1)
        .min(points.len() - 1);
    if index + 1 >= points.len() {
        return points[index];
    }
    let span = offsets[index + 1] - offsets[index];
    let fraction = if span > 0.0 {
        (offset - offsets[index]) / span
    } else {
        0.0
    };
    interpolate(points[index], points[index + 1], fraction)
}
