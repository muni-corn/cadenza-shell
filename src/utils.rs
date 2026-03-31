pub mod icons;

pub fn median_of(iter: impl Iterator<Item = f64>) -> f64 {
    let mut values = iter.collect::<Vec<_>>();

    if values.is_empty() {
        return f64::default();
    }

    values.sort_by(f64::total_cmp);

    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}
