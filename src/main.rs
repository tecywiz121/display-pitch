mod buffer;

use buffer::Consumer;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, SampleRate, StreamError,
};
use iced::{
    futures::{channel::mpsc, SinkExt, Stream, StreamExt},
    stream,
    widget::{column, text, Column},
    window, Font, Length, Size, Subscription, Task,
};
use pitch_detector::{
    note::{detect_note_in_range, NoteDetectionResult},
    pitch::HannedFftDetector,
};
use snafu::{OptionExt, Snafu};

#[derive(Snafu, Debug)]
enum Error {
    NoInput,
    NoConfig,
    #[snafu(context(false))]
    PlayStream {
        source: cpal::PlayStreamError,
    },
    #[snafu(context(false))]
    BuildStream {
        source: cpal::BuildStreamError,
    },
    #[snafu(context(false))]
    SupportedStreamConfigs {
        source: cpal::SupportedStreamConfigsError,
    },
    #[snafu(context(false))]
    DefaultStreamConfig {
        source: cpal::DefaultStreamConfigError,
    },
    #[snafu(context(false))]
    DeviceName {
        source: cpal::DeviceNameError,
    },
    #[snafu(context(false))]
    Iced {
        source: iced::Error,
    },
}

#[derive(Default)]
struct State {
    text: String,
}

impl State {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::NewText(txt) => self.text = txt,
        }

        Task::none()
    }

    fn view(&self) -> Column<Message> {
        column![text(&self.text).size(50).font(Font::MONOSPACE)]
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run(setup2)
    }
}

#[derive(Debug, Clone)]
enum Message {
    NewText(String),
}

const SAMPLE_RATE: SampleRate = SampleRate(44_100);

fn on_error(e: StreamError) {
    eprintln!("Input stream error: {}", e);
}

fn detect(
    mut consumer: Consumer<f32>,
    mut note_sender: mpsc::Sender<NoteDetectionResult>,
) {
    let mut upscale = Vec::<f64>::with_capacity(4096);
    let mut detector = HannedFftDetector::default();

    while let Ok(signal) = consumer.read(4096) {
        upscale.clear();
        upscale.extend(signal.into_iter().map(|x| x as f64));

        let result = detect_note_in_range(
            &upscale,
            &mut detector,
            SAMPLE_RATE.0 as _,
            50.0..440.0,
        );
        let note = match result {
            Some(f) => f,
            _ => continue,
        };

        if note_sender.try_send(note).is_err() {
            break;
        }
    }
}

fn setup2() -> impl Stream<Item = Message> {
    stream::channel(1024, |mut output| async move {
        let mut receiver = setup().unwrap();
        while let Some(r) = receiver.next().await {
            let text = format!(
                "{:3} {}",
                r.note_name.to_string(),
                r.actual_freq.round()
            );
            if output.send(Message::NewText(text)).await.is_err() {
                break;
            }
        }
    })
}

fn setup() -> Result<mpsc::Receiver<NoteDetectionResult>, Error> {
    let host = cpal::default_host();
    let device = host.default_input_device().context(NoInputSnafu)?;
    println!("Input device: {}", device.name()?);

    let config = device
        .supported_input_configs()?
        .filter(|c| c.sample_format() == SampleFormat::F32)
        .filter(|c| c.channels() >= 1)
        .filter(|c| c.min_sample_rate() <= SAMPLE_RATE)
        .filter(|c| c.max_sample_rate() >= SAMPLE_RATE)
        .min_by_key(|c| c.channels())
        .context(NoConfigSnafu)?
        .with_sample_rate(SAMPLE_RATE);

    println!("Input Configuration: {:#?}", config);

    let (producer, consumer) = buffer::new::<f32>();
    let (note_sender, note_receiver) = mpsc::channel(1024);

    let stream = device.build_input_stream(
        &config.into(),
        move |data, _info| producer.write(data),
        on_error,
        None,
    )?;

    stream.play()?;
    std::mem::forget(stream);

    let _detect_thread =
        std::thread::spawn(move || detect(consumer, note_sender));

    Ok(note_receiver)
}

fn main() -> Result<(), Error> {
    iced::application("Pitch Display", State::update, State::view)
        .window_size(Size::new(300., 75.))
        .subscription(State::subscription)
        .transparent(true)
        .resizable(false)
        .run()?;

    // TODO: Close stream
    // TODO: Join thread handle
    Ok(())
}
