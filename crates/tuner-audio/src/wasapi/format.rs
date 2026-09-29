//! WAVEFORMATEX parsing/building and sample conversion.

use windows::Win32::Media::Audio::{
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE, WAVEFORMATEXTENSIBLE_0, WAVE_FORMAT_PCM,
};
use windows::Win32::Media::KernelStreaming::{KSDATAFORMAT_SUBTYPE_PCM, WAVE_FORMAT_EXTENSIBLE};
use windows::Win32::Media::Multimedia::{KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, WAVE_FORMAT_IEEE_FLOAT};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SampleKind {
    F32,
    /// 32-bit container (valid bits may be 24).
    I32,
    /// Packed 24-bit.
    I24,
    I16,
}

impl SampleKind {
    fn bytes(self) -> usize {
        match self {
            SampleKind::F32 | SampleKind::I32 => 4,
            SampleKind::I24 => 3,
            SampleKind::I16 => 2,
        }
    }
}

/// A parsed stream format plus the raw struct to hand back to WASAPI.
#[derive(Clone, Copy)]
pub(crate) struct Format {
    pub rate: u32,
    pub channels: u16,
    pub kind: SampleKind,
    pub raw: WAVEFORMATEXTENSIBLE,
}

impl core::fmt::Debug for Format {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Format")
            .field("rate", &self.rate)
            .field("channels", &self.channels)
            .field("kind", &self.kind)
            .finish()
    }
}

fn channel_mask(channels: u16) -> u32 {
    match channels {
        1 => 0x4,       // SPEAKER_FRONT_CENTER
        2 => 0x1 | 0x2, // FRONT_LEFT | FRONT_RIGHT
        _ => 0,         // KSAUDIO_SPEAKER_DIRECTOUT
    }
}

impl Format {
    /// Build a WAVE_FORMAT_EXTENSIBLE format.
    pub(crate) fn new(rate: u32, channels: u16, kind: SampleKind, valid_bits: u16) -> Self {
        let bytes = kind.bytes() as u16;
        let block_align = bytes * channels;
        let raw = WAVEFORMATEXTENSIBLE {
            Format: WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
                nChannels: channels,
                nSamplesPerSec: rate,
                nAvgBytesPerSec: rate * u32::from(block_align),
                nBlockAlign: block_align,
                wBitsPerSample: bytes * 8,
                cbSize: 22,
            },
            Samples: WAVEFORMATEXTENSIBLE_0 {
                wValidBitsPerSample: valid_bits,
            },
            dwChannelMask: channel_mask(channels),
            SubFormat: if kind == SampleKind::F32 {
                KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
            } else {
                KSDATAFORMAT_SUBTYPE_PCM
            },
        };
        Self {
            rate,
            channels,
            kind,
            raw,
        }
    }

    /// Parse a format returned by WASAPI.
    ///
    /// # Safety
    /// `p` must point to a valid WAVEFORMATEX (and to a full
    /// WAVEFORMATEXTENSIBLE when its tag says so).
    pub(crate) unsafe fn from_ptr(p: *const WAVEFORMATEX) -> Option<Self> {
        if p.is_null() {
            return None;
        }
        // SAFETY: caller guarantees `p` is valid; read_unaligned copes with
        // the packed(1) layout.
        let wf = unsafe { p.read_unaligned() };
        let tag = u32::from(wf.wFormatTag);
        let bits = wf.wBitsPerSample;
        let (sub, valid, raw) = if tag == WAVE_FORMAT_EXTENSIBLE && wf.cbSize >= 22 {
            // SAFETY: tag + cbSize say the extensible tail is present.
            let ext = unsafe { (p as *const WAVEFORMATEXTENSIBLE).read_unaligned() };
            // SAFETY: reading a u16 from a union of u16s.
            let valid = unsafe { ext.Samples.wValidBitsPerSample };
            (ext.SubFormat, valid, ext)
        } else {
            let sub = if tag == WAVE_FORMAT_IEEE_FLOAT {
                KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
            } else if tag == WAVE_FORMAT_PCM {
                KSDATAFORMAT_SUBTYPE_PCM
            } else {
                return None;
            };
            let f = Self::new(wf.nSamplesPerSec, wf.nChannels, SampleKind::I16, bits);
            let mut raw = f.raw;
            raw.Format = wf;
            (sub, bits, raw)
        };
        let kind = match (sub, bits) {
            (s, 32) if s == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT => SampleKind::F32,
            (s, 32) if s == KSDATAFORMAT_SUBTYPE_PCM => SampleKind::I32,
            (s, 24) if s == KSDATAFORMAT_SUBTYPE_PCM => SampleKind::I24,
            (s, 16) if s == KSDATAFORMAT_SUBTYPE_PCM => SampleKind::I16,
            _ => return None,
        };
        let _ = valid;
        Some(Self {
            rate: wf.nSamplesPerSec,
            channels: wf.nChannels.max(1),
            kind,
            raw,
        })
    }

    pub(crate) fn as_ptr(&self) -> *const WAVEFORMATEX {
        &self.raw as *const WAVEFORMATEXTENSIBLE as *const WAVEFORMATEX
    }

    pub(crate) fn frame_bytes(&self) -> usize {
        self.kind.bytes() * usize::from(self.channels)
    }

    /// Exclusive-mode candidates in preference order: float32, then 24-in-32,
    /// then int16 (Architecture › Capture step 2), at the preferred rate first.
    pub(crate) fn exclusive_candidates(preferred_rate: u32, mix: &Format) -> Vec<Format> {
        let mut rates = vec![preferred_rate];
        if mix.rate != preferred_rate {
            rates.push(mix.rate);
        }
        let mut chans = vec![mix.channels];
        for c in [1, 2] {
            if !chans.contains(&c) {
                chans.push(c);
            }
        }
        let mut out = Vec::new();
        for &r in &rates {
            for &c in &chans {
                out.push(Format::new(r, c, SampleKind::F32, 32));
                out.push(Format::new(r, c, SampleKind::I32, 24));
                out.push(Format::new(r, c, SampleKind::I32, 32));
                out.push(Format::new(r, c, SampleKind::I16, 16));
            }
        }
        out
    }
}

/// Convert one interleaved device buffer to mono using `channel`.
///
/// # Safety
/// `src` must point to `frames * fmt.frame_bytes()` readable bytes.
pub(crate) unsafe fn to_mono(
    src: *const u8,
    frames: usize,
    fmt: &Format,
    channel: usize,
    dst: &mut [f32],
) {
    let n = frames.min(dst.len());
    let fb = fmt.frame_bytes();
    let sb = fmt.kind.bytes();
    let ch = channel.min(usize::from(fmt.channels) - 1);
    for (i, d) in dst[..n].iter_mut().enumerate() {
        // SAFETY: i < frames, so the sample lies inside the caller's buffer.
        let p = unsafe { src.add(i * fb + ch * sb) };
        // SAFETY: `p` points to `sb` readable bytes (see above).
        *d = unsafe { read_sample(p, fmt.kind) };
    }
}

/// Write mono samples to every channel of an interleaved device buffer.
///
/// # Safety
/// `dst` must point to `src.len() * fmt.frame_bytes()` writable bytes.
pub(crate) unsafe fn from_mono(src: &[f32], dst: *mut u8, fmt: &Format) {
    let fb = fmt.frame_bytes();
    let sb = fmt.kind.bytes();
    for (i, &s) in src.iter().enumerate() {
        for c in 0..usize::from(fmt.channels) {
            // SAFETY: inside the caller's buffer for frame i, channel c.
            unsafe { write_sample(dst.add(i * fb + c * sb), fmt.kind, s) };
        }
    }
}

#[inline]
unsafe fn read_sample(p: *const u8, kind: SampleKind) -> f32 {
    // SAFETY: caller guarantees `p` has `kind.bytes()` readable bytes.
    unsafe {
        match kind {
            SampleKind::F32 => (p as *const f32).read_unaligned(),
            SampleKind::I32 => (p as *const i32).read_unaligned() as f32 / 2_147_483_648.0,
            SampleKind::I16 => f32::from((p as *const i16).read_unaligned()) / 32_768.0,
            SampleKind::I24 => {
                let b = [0, *p, *p.add(1), *p.add(2)];
                i32::from_le_bytes(b) as f32 / 2_147_483_648.0
            }
        }
    }
}

#[inline]
unsafe fn write_sample(p: *mut u8, kind: SampleKind, s: f32) {
    let s = s.clamp(-1.0, 1.0);
    // SAFETY: caller guarantees `p` has `kind.bytes()` writable bytes.
    unsafe {
        match kind {
            SampleKind::F32 => (p as *mut f32).write_unaligned(s),
            SampleKind::I32 => {
                (p as *mut i32).write_unaligned((f64::from(s) * 2_147_483_647.0) as i32)
            }
            SampleKind::I16 => (p as *mut i16).write_unaligned((s * 32_767.0) as i16),
            SampleKind::I24 => {
                let v = ((f64::from(s) * 8_388_607.0) as i32).to_le_bytes();
                *p = v[0];
                *p.add(1) = v[1];
                *p.add(2) = v[2];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_kinds() {
        for kind in [
            SampleKind::F32,
            SampleKind::I32,
            SampleKind::I24,
            SampleKind::I16,
        ] {
            let fmt = Format::new(48_000, 2, kind, 24);
            let src = [0.0f32, 0.5, -0.5, 0.999];
            let mut buf = vec![0u8; src.len() * fmt.frame_bytes()];
            let mut back = [0.0f32; 4];
            // SAFETY: buffer sized for 4 frames of `fmt`.
            unsafe {
                from_mono(&src, buf.as_mut_ptr(), &fmt);
                to_mono(buf.as_ptr(), 4, &fmt, 1, &mut back);
            }
            for (a, b) in src.iter().zip(back) {
                assert!((a - b).abs() < 1e-3, "{kind:?}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn parse_built_format() {
        let f = Format::new(44_100, 1, SampleKind::I16, 16);
        // SAFETY: pointer to a live WAVEFORMATEXTENSIBLE.
        let p = unsafe { Format::from_ptr(f.as_ptr()) }.unwrap();
        assert_eq!((p.rate, p.channels, p.kind), (44_100, 1, SampleKind::I16));
    }
}
