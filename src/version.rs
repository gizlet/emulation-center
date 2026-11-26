//! Version order, as the Gizlet Store ranks releases: SemVer precedence over
//! any number of numeric parts, so `1.22.2.1` follows `1.22.2`.

use std::cmp::Ordering;

fn split(v: &str) -> (Vec<u64>, Option<Vec<&str>>) {
    let v = v.strip_prefix('v').unwrap_or(v);
    let v = v.split('+').next().unwrap_or("");
    let (core, pre) = match v.split_once('-') {
        Some((core, pre)) => (core, Some(pre.split('.').collect())),
        None => (v, None),
    };
    (core.split('.').map(|p| p.parse().unwrap_or(0)).collect(), pre)
}

pub fn compare(a: &str, b: &str) -> Ordering {
    let (x, xp) = split(a);
    let (y, yp) = split(b);
    for i in 0..x.len().max(y.len()) {
        match x.get(i).unwrap_or(&0).cmp(y.get(i).unwrap_or(&0)) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    match (xp, yp) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(p), Some(q)) => {
            for i in 0..p.len().max(q.len()) {
                let (Some(a), Some(b)) = (p.get(i), q.get(i)) else {
                    return p.len().cmp(&q.len());
                };
                if a == b {
                    continue;
                }
                return match (a.parse::<u64>(), b.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    _ => a.cmp(b),
                };
            }
            Ordering::Equal
        }
    }
}

/// True when `latest` is a newer release than `installed`.
pub fn is_newer(latest: &str, installed: &str) -> bool {
    compare(latest, installed) == Ordering::Greater
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_like_the_store() {
        assert!(is_newer("1.22.2.1", "1.22.2"));
        assert!(is_newer("1.22.3", "1.22.2.9"));
        assert!(is_newer("1.2.4", "1.2.4-rc.10"));
        assert!(is_newer("1.2.4-rc.10", "1.2.4-rc.2"));
        assert!(!is_newer("v1.20.4.1", "1.20.4.1"));
        assert!(!is_newer("1.20.4", "1.20.4.1"));
    }
}
