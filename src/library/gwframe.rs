//! The "gwframe" module's helpers: a small numeric dataframe (pandas-like). The
//! builtins live in `call()`; this holds the frame store and CSV/selection helpers.
use std::cell::RefCell;

pub(super) struct Frame {
    pub names: Vec<String>,
    pub cols: Vec<Vec<f64>>, // every column has the same length (the row count)
}

thread_local! {
    pub(super) static FRAMES: RefCell<Vec<Option<Frame>>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn frame_push(f: Frame) -> i64 {
    FRAMES.with(|r| {
        let mut v = r.borrow_mut();
        v.push(Some(f));
        (v.len() - 1) as i64
    })
}

pub(super) fn frame_col<'a>(f: &'a Frame, name: &str) -> Result<&'a Vec<f64>, String> {
    f.names
        .iter()
        .position(|n| n == name)
        .map(|i| &f.cols[i])
        .ok_or_else(|| format!("gwframe: no column \"{}\"", name))
}

pub(super) fn with_frame<R>(h: i64, f: impl FnOnce(&Frame) -> Result<R, String>) -> Result<R, String> {
    FRAMES.with(|r| {
        let v = r.borrow();
        let fr = usize::try_from(h)
            .ok()
            .and_then(|i| v.get(i))
            .and_then(|o| o.as_ref())
            .ok_or("gwframe: invalid handle")?;
        f(fr)
    })
}

pub(super) fn frame_rows(f: &Frame) -> usize {
    f.cols.first().map_or(0, Vec::len)
}

pub(super) fn frame_from_csv(path: &str) -> Result<Frame, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("gwframe.from_csv: cannot read {}: {}", path, e))?;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or("gwframe.from_csv: the file is empty")?;
    let names: Vec<String> = header.split(',').map(|s| s.trim().to_string()).collect();
    let mut cols: Vec<Vec<f64>> = vec![Vec::new(); names.len()];
    for (li, line) in lines.enumerate() {
        let cells: Vec<&str> = line.split(',').collect();
        if cells.len() != names.len() {
            return Err(format!("gwframe.from_csv: row {} has {} cells, expected {}", li + 2, cells.len(), names.len()));
        }
        for (i, c) in cells.iter().enumerate() {
            let v = c
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("gwframe.from_csv: \"{}\" in row {} is not a number", c.trim(), li + 2))?;
            cols[i].push(v);
        }
    }
    Ok(Frame { names, cols })
}

pub(super) fn frame_select(f: &Frame, keep: &[bool]) -> Frame {
    let cols = f
        .cols
        .iter()
        .map(|c| c.iter().zip(keep).filter(|(_, &k)| k).map(|(&x, _)| x).collect())
        .collect();
    Frame { names: f.names.clone(), cols }
}

pub(super) fn frame_reorder(f: &Frame, order: &[usize]) -> Frame {
    let cols = f.cols.iter().map(|c| order.iter().map(|&i| c[i]).collect()).collect();
    Frame { names: f.names.clone(), cols }
}
