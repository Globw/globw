//! The "chart" module's helpers: tiny SVG line/bar/scatter plots. The builtins
//! live in `call()`; this builds the SVG from the data.
use crate::vm::Value;
use super::{real, seq};


pub(super) enum ChartKind {
    Line,
    Bars,
    Scatter,
}

const CHART_W: f64 = 640.0;
const CHART_H: f64 = 400.0;
const CHART_M: f64 = 45.0;

pub(super) fn chart_range(d: &[f64]) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &v in d {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if (hi - lo).abs() < 1e-12 {
        hi = lo + 1.0;
    }
    (lo, hi)
}

pub(super) fn chart_header() -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n\
         <rect width=\"{w}\" height=\"{h}\" fill=\"white\"/>\n\
         <rect x=\"{m}\" y=\"{m}\" width=\"{pw}\" height=\"{ph}\" fill=\"none\" stroke=\"#ccc\"/>\n",
        w = CHART_W, h = CHART_H, m = CHART_M, pw = CHART_W - 2.0 * CHART_M, ph = CHART_H - 2.0 * CHART_M
    )
}

pub(super) fn chart_svg(data: &[f64], bars: bool) -> String {
    let (lo, hi) = chart_range(data);
    let n = data.len();
    let sx = |i: usize| CHART_M + (CHART_W - 2.0 * CHART_M) * i as f64 / (n.max(2) - 1) as f64;
    let sy = |v: f64| (CHART_H - CHART_M) - (v - lo) / (hi - lo) * (CHART_H - 2.0 * CHART_M);
    let mut s = chart_header();
    if bars {
        let bw = (CHART_W - 2.0 * CHART_M) / n as f64 * 0.7;
        for (i, &v) in data.iter().enumerate() {
            let cx = CHART_M + (CHART_W - 2.0 * CHART_M) * (i as f64 + 0.5) / n as f64;
            let top = sy(v);
            s.push_str(&format!(
                "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#4682b4\"/>\n",
                cx - bw / 2.0, top, bw, (CHART_H - CHART_M) - top
            ));
        }
    } else {
        let pts: Vec<String> = data.iter().enumerate().map(|(i, &v)| format!("{:.1},{:.1}", sx(i), sy(v))).collect();
        s.push_str(&format!("<polyline points=\"{}\" fill=\"none\" stroke=\"#4682b4\" stroke-width=\"2\"/>\n", pts.join(" ")));
    }
    s.push_str(&format!("<text x=\"4\" y=\"{:.0}\" font-size=\"11\" fill=\"#666\">{:.3}</text>\n", CHART_M + 4.0, hi));
    s.push_str(&format!("<text x=\"4\" y=\"{:.0}\" font-size=\"11\" fill=\"#666\">{:.3}</text>\n", CHART_H - CHART_M, lo));
    s.push_str("</svg>\n");
    s
}

pub(super) fn chart_svg_scatter(xs: &[f64], ys: &[f64]) -> String {
    let (xlo, xhi) = chart_range(xs);
    let (ylo, yhi) = chart_range(ys);
    let sx = |v: f64| CHART_M + (v - xlo) / (xhi - xlo) * (CHART_W - 2.0 * CHART_M);
    let sy = |v: f64| (CHART_H - CHART_M) - (v - ylo) / (yhi - ylo) * (CHART_H - 2.0 * CHART_M);
    let mut s = chart_header();
    for (&x, &y) in xs.iter().zip(ys) {
        s.push_str(&format!("<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"3\" fill=\"#4682b4\"/>\n", sx(x), sy(y)));
    }
    s.push_str("</svg>\n");
    s
}

pub(super) fn chart_write(xs: &Value, ys: Option<&Value>, kind: ChartKind, path: &str) -> Result<Value, String> {
    let data: Vec<f64> = seq(xs).borrow().iter().map(real).collect();
    if data.is_empty() {
        return Err("chart: the data is empty".to_string());
    }
    let svg = match kind {
        ChartKind::Line => chart_svg(&data, false),
        ChartKind::Bars => chart_svg(&data, true),
        ChartKind::Scatter => {
            let yd: Vec<f64> = seq(ys.expect("scatter has y")).borrow().iter().map(real).collect();
            if yd.len() != data.len() {
                return Err("chart.scatter: the two lists must have the same length".to_string());
            }
            chart_svg_scatter(&data, &yd)
        }
    };
    std::fs::write(path, svg).map_err(|e| format!("chart: cannot write \"{}\": {}", path, e))?;
    Ok(Value::Null)
}

