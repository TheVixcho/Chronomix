use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use cpal::{
    BufferSize, Device, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, StreamTrait},
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::{AudioError, AudioStats, find_device, pick_sample_rate, resolve_buffer_size};

const MAX_FEEDS: usize = 32;

#[derive(Debug, Clone)]
pub struct MixerAudioConfig {
    pub input_devices: Vec<String>,
    pub output_device: String,
    pub preferred_sample_rate: u32,
    pub block_size: usize,
}

#[derive(Debug, Clone)]
pub struct ActiveMixerInfo {
    pub output_sample_rate: u32,
    pub output_channels: u16,
    pub block_size: usize,
    pub feed_sample_rates: Vec<u32>,
    pub feed_channels: Vec<u16>,
}

pub trait MixerAudioProcessor: Send + 'static {
    fn process(
        &mut self,
        feeds: &[&[f32]],
        feed_channels: &[usize],
        out_left: &mut [f32],
        out_right: &mut [f32],
        frames: usize,
        sample_rate: f64,
    );
}

pub struct MixerAudioSession {
    _input_streams: Vec<Stream>,
    _output_stream: Stream,
    pub info: ActiveMixerInfo,
    pub stats: Arc<AudioStats>,
}

struct LinearMultiResampler {
    channels: usize,
    ratio: f64,
    phase: f64,
    prev: Vec<f32>,
    curr: Vec<f32>,
    initialized: bool,
}

impl LinearMultiResampler {
    fn new(channels: usize, in_sr: u32, out_sr: u32) -> Self {
        let ch = channels.max(1);
        Self {
            channels: ch,
            ratio: in_sr as f64 / out_sr.max(1) as f64,
            phase: 0.0,
            prev: vec![0.0; ch],
            curr: vec![0.0; ch],
            initialized: false,
        }
    }

    fn is_identity(&self) -> bool {
        (self.ratio - 1.0).abs() < 1e-7
    }

    fn push_frame(&mut self, frame: &[f32], producer: &mut Producer<f32>, overruns: &AtomicU64) {
        let ch = self.channels;
        if self.is_identity() {
            if producer.slots() >= ch {
                for &s in &frame[..ch] {
                    let _ = producer.push(s);
                }
            } else {
                overruns.fetch_add(1, Ordering::Relaxed);
            }
            return;
        }

        if !self.initialized {
            self.prev.copy_from_slice(&frame[..ch]);
            self.curr.copy_from_slice(&frame[..ch]);
            self.initialized = true;
            return;
        }

        self.prev.copy_from_slice(&self.curr);
        self.curr.copy_from_slice(&frame[..ch]);

        while self.phase < 1.0 {
            let t = self.phase as f32;
            if producer.slots() >= ch {
                for c in 0..ch {
                    let out = self.prev[c] + (self.curr[c] - self.prev[c]) * t;
                    let _ = producer.push(out);
                }
            } else {
                overruns.fetch_add(1, Ordering::Relaxed);
            }
            self.phase += self.ratio;
        }
        self.phase -= 1.0;
    }
}

struct MultiInputBridge {
    producer: Producer<f32>,
    in_channels: usize,
    resampler: LinearMultiResampler,
    stats: Arc<AudioStats>,
    scratch_frame: Vec<f32>,
}

impl MultiInputBridge {
    fn new(
        producer: Producer<f32>,
        in_channels: usize,
        in_sr: u32,
        out_sr: u32,
        stats: Arc<AudioStats>,
    ) -> Self {
        let ch = in_channels.max(1);
        Self {
            producer,
            in_channels: ch,
            resampler: LinearMultiResampler::new(ch, in_sr, out_sr),
            stats,
            scratch_frame: vec![0.0; ch],
        }
    }

    fn push_samples<T: Sample>(&mut self, data: &[T])
    where
        f32: cpal::FromSample<T>,
    {
        self.stats.input_callbacks.fetch_add(1, Ordering::Relaxed);
        let ch = self.in_channels;
        for frame in data.chunks_exact(ch) {
            for (dst, &src) in self.scratch_frame.iter_mut().zip(frame.iter()) {
                *dst = src.to_sample();
            }
            self.resampler
                .push_frame(&self.scratch_frame, &mut self.producer, &self.stats.input_overruns);
        }
    }
}

struct FeedReader {
    consumer: Consumer<f32>,
    channels: usize,
    primed: bool,
    buf: Vec<f32>,
}

impl FeedReader {
    fn new(consumer: Consumer<f32>, channels: usize, max_frames: usize) -> Self {
        let ch = channels.max(1);
        Self {
            consumer,
            channels: ch,
            primed: false,
            buf: vec![0.0; max_frames * ch],
        }
    }

    fn pull_chunk(&mut self, chunk_frames: usize, underruns: &AtomicU64) {
        let ch = self.channels;
        let needed = chunk_frames * ch;
        let mut available_frames = self.consumer.slots() / ch;

        let max_buffered = (chunk_frames * 5).max(1536);
        let target_keep = chunk_frames + (chunk_frames / 2).min(512);
        if available_frames > max_buffered {
            let drop_frames = available_frames.saturating_sub(target_keep);
            for _ in 0..(drop_frames * ch) {
                let _ = self.consumer.pop();
            }
            available_frames = self.consumer.slots() / ch;
        }

        let prime_threshold = if chunk_frames >= 32 {
            target_keep
        } else {
            chunk_frames
        };

        if !self.primed && available_frames >= prime_threshold {
            self.primed = true;
        }

        if self.primed && available_frames >= chunk_frames {
            for dst in &mut self.buf[..needed] {
                *dst = self.consumer.pop().unwrap_or(0.0);
            }
        } else {
            if self.primed {
                self.primed = false;
                underruns.fetch_add(1, Ordering::Relaxed);
            }
            self.buf[..needed].fill(0.0);
        }
    }
}

struct MixerOutputBridge<P: MixerAudioProcessor> {
    feeds: Vec<FeedReader>,
    processor: P,
    out_channels: usize,
    out_sample_rate: f64,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    stats: Arc<AudioStats>,
}

impl<P: MixerAudioProcessor> MixerOutputBridge<P> {
    fn render_samples<T: Sample + cpal::FromSample<f32>>(&mut self, output: &mut [T]) {
        self.stats.output_callbacks.fetch_add(1, Ordering::Relaxed);
        let ch = self.out_channels.max(1);
        let total_frames = output.len() / ch;
        if total_frames == 0 {
            return;
        }

        let max_chunk = self.out_l.len();
        let active_feeds = self.feeds.len().min(MAX_FEEDS);
        let mut frame_offset = 0;

        while frame_offset < total_frames {
            let chunk_frames = (total_frames - frame_offset).min(max_chunk);
            for feed in &mut self.feeds[..active_feeds] {
                feed.pull_chunk(chunk_frames, &self.stats.output_underruns);
            }

            let mut feed_slices: [&[f32]; MAX_FEEDS] = [&[]; MAX_FEEDS];
            let mut feed_channels: [usize; MAX_FEEDS] = [0; MAX_FEEDS];
            for (idx, feed) in self.feeds[..active_feeds].iter().enumerate() {
                feed_slices[idx] = &feed.buf[..chunk_frames * feed.channels];
                feed_channels[idx] = feed.channels;
            }

            self.out_l[..chunk_frames].fill(0.0);
            self.out_r[..chunk_frames].fill(0.0);

            self.processor.process(
                &feed_slices[..active_feeds],
                &feed_channels[..active_feeds],
                &mut self.out_l[..chunk_frames],
                &mut self.out_r[..chunk_frames],
                chunk_frames,
                self.out_sample_rate,
            );

            let out_slice = &mut output[frame_offset * ch..(frame_offset + chunk_frames) * ch];
            for (i, frame) in out_slice.chunks_exact_mut(ch).enumerate() {
                let l = self.out_l[i];
                let r = self.out_r[i];
                if ch == 1 {
                    frame[0] = T::from_sample((l + r) * 0.5);
                } else {
                    frame[0] = T::from_sample(l);
                    frame[1] = T::from_sample(r);
                    for extra in frame.iter_mut().skip(2) {
                        *extra = T::from_sample(0.0);
                    }
                }
            }

            frame_offset += chunk_frames;
        }
    }
}

fn build_typed_multi_input<T: SizedSample>(
    device: &Device,
    config: &StreamConfig,
    bridge: MultiInputBridge,
) -> Result<Stream, AudioError>
where
    f32: cpal::FromSample<T>,
{
    let slot = Arc::new(Mutex::new(Some(bridge)));
    let make_cb = |slot: Arc<Mutex<Option<MultiInputBridge>>>| {
        let mut local: Option<MultiInputBridge> = None;
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let b = local.get_or_insert_with(|| {
                slot.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("mixer input bridge already taken")
            });
            b.push_samples(data);
        }
    };

    match device.build_input_stream(
        *config,
        make_cb(Arc::clone(&slot)),
        |err| log::error!("Mixer input stream error: {err}"),
        None,
    ) {
        Ok(stream) => Ok(stream),
        Err(_) if !matches!(config.buffer_size, BufferSize::Default) => {
            let mut fallback = *config;
            fallback.buffer_size = BufferSize::Default;
            device
                .build_input_stream(
                    fallback,
                    make_cb(slot),
                    |err| log::error!("Mixer input stream error: {err}"),
                    None,
                )
                .map_err(|e| AudioError::BuildStream(format!("Input stream: {e}")))
        }
        Err(e) => Err(AudioError::BuildStream(format!("Input stream: {e}"))),
    }
}

fn build_multi_input_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    bridge: MultiInputBridge,
) -> Result<Stream, AudioError> {
    match format {
        SampleFormat::F32 => build_typed_multi_input::<f32>(device, config, bridge),
        SampleFormat::I16 => build_typed_multi_input::<i16>(device, config, bridge),
        SampleFormat::I32 => build_typed_multi_input::<i32>(device, config, bridge),
        SampleFormat::U16 => build_typed_multi_input::<u16>(device, config, bridge),
        other => Err(AudioError::UnsupportedFormat(other)),
    }
}

fn build_typed_mixer_output<T: SizedSample + cpal::FromSample<f32>, P: MixerAudioProcessor>(
    device: &Device,
    config: &StreamConfig,
    bridge: MixerOutputBridge<P>,
) -> Result<Stream, AudioError> {
    let slot = Arc::new(Mutex::new(Some(bridge)));
    let make_cb = |slot: Arc<Mutex<Option<MixerOutputBridge<P>>>>| {
        let mut local: Option<MixerOutputBridge<P>> = None;
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let b = local.get_or_insert_with(|| {
                slot.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("mixer output bridge already taken")
            });
            b.render_samples(data);
        }
    };

    match device.build_output_stream(
        *config,
        make_cb(Arc::clone(&slot)),
        |err| log::error!("Mixer output stream error: {err}"),
        None,
    ) {
        Ok(stream) => Ok(stream),
        Err(_) if !matches!(config.buffer_size, BufferSize::Default) => {
            let mut fallback = *config;
            fallback.buffer_size = BufferSize::Default;
            device
                .build_output_stream(
                    fallback,
                    make_cb(slot),
                    |err| log::error!("Mixer output stream error: {e}", e = err),
                    None,
                )
                .map_err(|e| AudioError::BuildStream(format!("Output stream: {e}")))
        }
        Err(e) => Err(AudioError::BuildStream(format!("Output stream: {e}"))),
    }
}

fn build_mixer_output_stream<P: MixerAudioProcessor>(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    bridge: MixerOutputBridge<P>,
) -> Result<Stream, AudioError> {
    match format {
        SampleFormat::F32 => build_typed_mixer_output::<f32, P>(device, config, bridge),
        SampleFormat::I16 => build_typed_mixer_output::<i16, P>(device, config, bridge),
        SampleFormat::I32 => build_typed_mixer_output::<i32, P>(device, config, bridge),
        SampleFormat::U16 => build_typed_mixer_output::<u16, P>(device, config, bridge),
        other => Err(AudioError::UnsupportedFormat(other)),
    }
}

pub fn start_mixer_audio_session<P: MixerAudioProcessor>(
    config: &MixerAudioConfig,
    processor: P,
) -> Result<MixerAudioSession, AudioError> {
    let out_device = find_device(&config.output_device, false)?;
    let out_default = out_device
        .default_output_config()
        .map_err(|e| AudioError::Config(format!("Output config: {e}")))?;

    let out_sr = pick_sample_rate(&out_device, false, config.preferred_sample_rate, &out_default);
    let out_channels = out_default.channels().max(1);
    let out_stream_cfg = StreamConfig {
        channels: out_channels,
        sample_rate: out_sr,
        buffer_size: resolve_buffer_size(
            &out_device,
            false,
            out_sr,
            out_channels,
            config.block_size,
        ),
    };

    let stats = Arc::new(AudioStats::default());
    let max_scratch_frames = config.block_size.max(4096);
    let mut input_streams = Vec::new();
    let mut feed_readers = Vec::new();
    let mut feed_sample_rates = Vec::new();
    let mut feed_channels = Vec::new();

    for in_id in config.input_devices.iter().take(MAX_FEEDS) {
        let in_device = find_device(in_id, true)?;
        let in_default = in_device
            .default_input_config()
            .map_err(|e| AudioError::Config(format!("Input config: {e}")))?;
        let in_sr = pick_sample_rate(&in_device, true, out_sr, &in_default);
        let in_ch = in_default.channels().max(1);
        let in_stream_cfg = StreamConfig {
            channels: in_ch,
            sample_rate: in_sr,
            buffer_size: resolve_buffer_size(&in_device, true, in_sr, in_ch, config.block_size),
        };

        let ring_frames = (config.block_size * 8).max(4096);
        let (producer, consumer) = RingBuffer::<f32>::new(ring_frames * in_ch as usize);
        let in_bridge = MultiInputBridge::new(producer, in_ch as usize, in_sr, out_sr, Arc::clone(&stats));
        let in_stream = build_multi_input_stream(
            &in_device,
            &in_stream_cfg,
            in_default.sample_format(),
            in_bridge,
        )?;

        input_streams.push(in_stream);
        feed_readers.push(FeedReader::new(consumer, in_ch as usize, max_scratch_frames));
        feed_sample_rates.push(in_sr);
        feed_channels.push(in_ch);
    }

    let output_bridge = MixerOutputBridge {
        feeds: feed_readers,
        processor,
        out_channels: out_channels as usize,
        out_sample_rate: out_sr as f64,
        out_l: vec![0.0; max_scratch_frames],
        out_r: vec![0.0; max_scratch_frames],
        stats: Arc::clone(&stats),
    };

    let output_stream = build_mixer_output_stream(
        &out_device,
        &out_stream_cfg,
        out_default.sample_format(),
        output_bridge,
    )?;

    for s in &input_streams {
        s.play()
            .map_err(|e| AudioError::PlayStream(format!("Input play: {e}")))?;
    }
    output_stream
        .play()
        .map_err(|e| AudioError::PlayStream(format!("Output play: {e}")))?;

    Ok(MixerAudioSession {
        _input_streams: input_streams,
        _output_stream: output_stream,
        info: ActiveMixerInfo {
            output_sample_rate: out_sr,
            output_channels: out_channels,
            block_size: config.block_size,
            feed_sample_rates,
            feed_channels,
        },
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SumTwoFeedsProcessor;

    impl MixerAudioProcessor for SumTwoFeedsProcessor {
        fn process(
            &mut self,
            feeds: &[&[f32]],
            feed_channels: &[usize],
            out_left: &mut [f32],
            out_right: &mut [f32],
            frames: usize,
            _sample_rate: f64,
        ) {
            for i in 0..frames {
                let mut l = 0.0;
                let mut r = 0.0;
                for (feed, &ch) in feeds.iter().zip(feed_channels.iter()) {
                    let base = i * ch;
                    if base < feed.len() {
                        l += feed[base];
                        r += if ch >= 2 { feed[base + 1] } else { feed[base] };
                    }
                }
                out_left[i] = l;
                out_right[i] = r;
            }
        }
    }

    #[test]
    fn multi_feed_mixer_sums_multiple_devices_per_channel() {
        let (p1, c1) = RingBuffer::<f32>::new(256);
        let (p2, c2) = RingBuffer::<f32>::new(256);
        let stats = Arc::new(AudioStats::default());

        let mut in1 = MultiInputBridge::new(p1, 1, 48_000, 48_000, Arc::clone(&stats));
        let mut in2 = MultiInputBridge::new(p2, 2, 48_000, 48_000, Arc::clone(&stats));

        in1.push_samples(&[0.25_f32, 0.5_f32]);
        in2.push_samples(&[0.10_f32, -0.20_f32, 0.30_f32, -0.10_f32]);

        let mut out = MixerOutputBridge {
            feeds: vec![FeedReader::new(c1, 1, 64), FeedReader::new(c2, 2, 64)],
            processor: SumTwoFeedsProcessor,
            out_channels: 2,
            out_sample_rate: 48_000.0,
            out_l: vec![0.0; 64],
            out_r: vec![0.0; 64],
            stats,
        };

        let mut interleaved = [0.0_f32; 4];
        out.render_samples(&mut interleaved);
        assert!((interleaved[0] - 0.35).abs() < 1e-6);
        assert!((interleaved[1] - 0.05).abs() < 1e-6);
        assert!((interleaved[2] - 0.80).abs() < 1e-6);
        assert!((interleaved[3] - 0.40).abs() < 1e-6);
    }

    #[test]
    fn multi_channel_resampler_preserves_all_channels() {
        let (mut producer, mut consumer) = RingBuffer::<f32>::new(4096);
        let overruns = AtomicU64::new(0);
        let mut resampler = LinearMultiResampler::new(4, 44_100, 48_000);

        for i in 0..441 {
            let v = i as f32 / 441.0;
            resampler.push_frame(&[v, -v, v * 0.5, 0.25], &mut producer, &overruns);
        }

        let frames = consumer.slots() / 4;
        assert!((475..=485).contains(&frames));
        assert_eq!(overruns.load(Ordering::Relaxed), 0);
        let c0 = consumer.pop().unwrap();
        let c1 = consumer.pop().unwrap();
        let c2 = consumer.pop().unwrap();
        let c3 = consumer.pop().unwrap();
        assert!((c0 + c1).abs() < 1e-6);
        assert!(c2.abs() < 1e-3);
        assert!((c3 - 0.25).abs() < 1e-6);
    }
}
