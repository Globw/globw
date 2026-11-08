//! The "grad" module's helpers: a tape-based reverse-mode autodiff engine. The
//! builtins live in `call()`; this holds the tape and the backward pass.
//! Each operation appends a node whose inputs always have smaller indices, so one
//! reverse pass over the tape (high index to low) back-propagates the gradients.
use std::cell::RefCell;

#[derive(Clone, Copy)]
pub(super) struct GNode {
    value: f64,
    grad: f64,
    op: u8, // 0 leaf,1 add,2 sub,3 mul,4 div,5 neg,6 pow,7 exp,8 ln,9 sin,10 cos,11 tanh,12 sigmoid,13 relu
    a: usize,
    b: usize,
    k: f64, // constant parameter (pow exponent)
}

thread_local! {
    pub(super) static GRAD: RefCell<Vec<GNode>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn g_push(value: f64, op: u8, a: usize, b: usize, k: f64) -> i64 {
    GRAD.with(|g| {
        let mut v = g.borrow_mut();
        v.push(GNode { value, grad: 0.0, op, a, b, k });
        (v.len() - 1) as i64
    })
}

pub(super) fn g_val(h: i64) -> Result<f64, String> {
    GRAD.with(|g| {
        g.borrow()
            .get(usize::try_from(h).map_err(|_| "grad: invalid handle".to_string())?)
            .map(|n| n.value)
            .ok_or_else(|| "grad: invalid handle".to_string())
    })
}

pub(super) fn g_grad(h: i64) -> Result<f64, String> {
    GRAD.with(|g| {
        g.borrow()
            .get(usize::try_from(h).map_err(|_| "grad: invalid handle".to_string())?)
            .map(|n| n.grad)
            .ok_or_else(|| "grad: invalid handle".to_string())
    })
}

pub(super) fn g_backward(target: i64) -> Result<(), String> {
    GRAD.with(|g| {
        let mut v = g.borrow_mut();
        let t = usize::try_from(target).ok().filter(|&t| t < v.len()).ok_or("grad: invalid handle")?;
        for n in v.iter_mut() {
            n.grad = 0.0;
        }
        v[t].grad = 1.0;
        for i in (0..=t).rev() {
            let n = v[i];
            let gi = n.grad;
            match n.op {
                1 => {
                    v[n.a].grad += gi;
                    v[n.b].grad += gi;
                }
                2 => {
                    v[n.a].grad += gi;
                    v[n.b].grad -= gi;
                }
                3 => {
                    let (va, vb) = (v[n.a].value, v[n.b].value);
                    v[n.a].grad += gi * vb;
                    v[n.b].grad += gi * va;
                }
                4 => {
                    let (va, vb) = (v[n.a].value, v[n.b].value);
                    v[n.a].grad += gi / vb;
                    v[n.b].grad -= gi * va / (vb * vb);
                }
                5 => v[n.a].grad -= gi,
                6 => {
                    let va = v[n.a].value;
                    v[n.a].grad += gi * n.k * va.powf(n.k - 1.0);
                }
                7 => v[n.a].grad += gi * n.value,
                8 => {
                    let va = v[n.a].value;
                    v[n.a].grad += gi / va;
                }
                9 => {
                    let va = v[n.a].value;
                    v[n.a].grad += gi * va.cos();
                }
                10 => {
                    let va = v[n.a].value;
                    v[n.a].grad -= gi * va.sin();
                }
                11 => v[n.a].grad += gi * (1.0 - n.value * n.value),
                12 => v[n.a].grad += gi * n.value * (1.0 - n.value),
                13 => {
                    if v[n.a].value > 0.0 {
                        v[n.a].grad += gi;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })
}
