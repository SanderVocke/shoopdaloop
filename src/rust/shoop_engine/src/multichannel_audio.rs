use std::sync::Mutex;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MultichannelAudioError {
    #[error("invalid multichannel audio shape: {n_channels} channels, {n_frames} frames")]
    InvalidShape { n_channels: u32, n_frames: u32 },
    #[error("multichannel audio index out of range: frame {frame}, channel {channel}")]
    IndexOutOfRange { frame: u32, channel: u32 },
    #[error("resampling failed: {0}")]
    Resample(String),
    #[error("multichannel audio buffer lock poisoned")]
    LockPoisoned,
}

pub type Result<T> = std::result::Result<T, MultichannelAudioError>;

pub struct MultichannelAudio {
    n_channels: u32,
    n_frames: u32,
    data: Mutex<Vec<f32>>,
}

impl MultichannelAudio {
    pub fn new(n_channels: u32, n_frames: u32) -> Result<Self> {
        let len = n_channels
            .checked_mul(n_frames)
            .ok_or(MultichannelAudioError::InvalidShape {
                n_channels,
                n_frames,
            })?;
        Ok(Self {
            n_channels,
            n_frames,
            data: Mutex::new(vec![0.0; len as usize]),
        })
    }

    pub fn resample(&self, new_n_frames: u32) -> Result<Self> {
        let src = self
            .data
            .lock()
            .map_err(|_| MultichannelAudioError::LockPoisoned)?;
        let interleaved = src.clone();
        let out = crate::resample::resample_interleaved(
            &interleaved,
            self.n_channels as usize,
            new_n_frames as usize,
        )
        .map_err(|e| MultichannelAudioError::Resample(e.to_string()))?;
        let r = Self::new(self.n_channels, new_n_frames)?;
        {
            let mut dst = r
                .data
                .lock()
                .map_err(|_| MultichannelAudioError::LockPoisoned)?;
            *dst = out;
        }
        Ok(r)
    }

    pub fn at(&self, frame: u32, channel: u32) -> Result<f32> {
        let idx = self.index(frame, channel)?;
        Ok(self
            .data
            .lock()
            .map_err(|_| MultichannelAudioError::LockPoisoned)?[idx])
    }

    pub fn set(&self, frame: u32, channel: u32, value: f32) -> Result<()> {
        let idx = self.index(frame, channel)?;
        self.data
            .lock()
            .map_err(|_| MultichannelAudioError::LockPoisoned)?[idx] = value;
        Ok(())
    }

    fn index(&self, frame: u32, channel: u32) -> Result<usize> {
        if frame >= self.n_frames || channel >= self.n_channels {
            return Err(MultichannelAudioError::IndexOutOfRange { frame, channel });
        }
        Ok((frame * self.n_channels + channel) as usize)
    }
}
