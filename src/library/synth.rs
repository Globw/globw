//! The "synth" module's helpers: audio synthesis and simple DSP. The builtins
//! themselves live in `call()` (in the parent module); this holds the sample
//! buffers and the WAV encode/decode so the big dispatch file stays smaller.
use std::cell::{Cell, RefCell};

thread_local! {
    pub(super) static SYNTH: RefCell<Vec<Option<Vec<f32>>>> = const { RefCell::new(Vec::new()) };
    pub(super) static SRATE: Cell<u32> = const { Cell::new(44100) };
}

pub(super) fn syn_rate() -> u32 {
    SRATE.with(Cell::get)
}

pub(super) fn syn_push(buf: Vec<f32>) -> i64 {
    SYNTH.with(|r| {
        let mut v = r.borrow_mut();
        v.push(Some(buf));
        (v.len() - 1) as i64
    })
}

pub(super) fn syn_get<R>(h: i64, f: impl FnOnce(&Vec<f32>) -> R) -> Result<R, String> {
    SYNTH.with(|r| {
        let v = r.borrow();
        let b = usize::try_from(h).ok().and_then(|i| v.get(i)).and_then(|o| o.as_ref()).ok_or("synth: invalid handle")?;
        Ok(f(b))
    })
}

pub(super) fn syn_mut(h: i64, f: impl FnOnce(&mut Vec<f32>)) -> Result<(), String> {
    SYNTH.with(|r| {
        let mut v = r.borrow_mut();
        let b = usize::try_from(h).ok().and_then(|i| v.get_mut(i)).and_then(|o| o.as_mut()).ok_or("synth: invalid handle")?;
        f(b);
        Ok(())
    })
}

pub(super) fn syn_wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + samples.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

pub(super) fn syn_read_wav(path: &str) -> Result<(Vec<f32>, u32), String> {
    let b = std::fs::read(path).map_err(|e| format!("synth.load_wav: cannot read {}: {}", path, e))?;
    if b.len() < 44 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("synth.load_wav: not a WAV file".to_string());
    }
    let rate = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);
    // find the "data" chunk
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let len = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        if id == b"data" {
            let end = (i + 8 + len).min(b.len());
            let samples = b[i + 8..end].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
            return Ok((samples, rate.max(1)));
        }
        i += 8 + len + (len & 1);
    }
    Err("synth.load_wav: no data chunk".to_string())
}
