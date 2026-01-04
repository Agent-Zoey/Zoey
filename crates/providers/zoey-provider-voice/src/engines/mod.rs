//! Voice engine implementations (TTS and STT)

// TTS engines
pub mod elevenlabs;
pub mod local;
pub mod openai;
pub mod piper;
pub mod supertonic;

// STT engines
#[cfg(feature = "whisper")]
pub mod whisper;
#[cfg(feature = "vosk-stt")]
pub mod vosk;
#[cfg(feature = "unmute")]
pub mod unmute;
#[cfg(feature = "unmute")]
pub mod unmute_realtime;
#[cfg(feature = "unmute")]
pub mod unmute_server;
#[cfg(feature = "unmute")]
pub mod unmute_dockerless;
#[cfg(feature = "moshi")]
pub mod moshi;
#[cfg(feature = "moshi")]
pub mod moshi_server;

// Local realtime pipeline (true low-latency)
#[cfg(feature = "whisper")]
pub mod local_realtime;

// TTS exports
pub use elevenlabs::ElevenLabsVoiceEngine;
pub use local::LocalVoiceEngine;
pub use openai::OpenAIVoiceEngine;
pub use piper::{
    PiperEngine, PiperVoice, PiperQuality,
    LocalPiperEngine, EmbeddedPiperConfig,
    synthesize_with_piper, pcm_to_wav,
};
pub use supertonic::{
    SupertonicEngine, SupertonicVoice, SupertonicParams,
    LocalSupertonicEngine, LocalSupertonicConfig, SupertonicPreset,
    SupertonicServer, SupertonicServerConfig, start_supertonic_server,
};

// STT exports
#[cfg(feature = "whisper")]
pub use whisper::WhisperEngine;
#[cfg(feature = "vosk-stt")]
pub use vosk::{VoskEngine, VoskModel};
#[cfg(feature = "unmute")]
pub use unmute::UnmuteEngine;
#[cfg(feature = "unmute")]
pub use unmute_realtime::{UnmuteRealtime, UnmuteTTS, VoiceConversation};
#[cfg(feature = "unmute")]
pub use unmute_server::UnmuteServer;
#[cfg(feature = "unmute")]
pub use unmute_dockerless::{UnmuteDockerless, UnmuteDockerlessBuilder, ServiceType, start_unmute_dockerless};
#[cfg(feature = "moshi")]
pub use moshi::{
    MoshiEngine, MoshiConfig, MoshiSessionConfig, MoshiModel,
    MoshiStreamingClient, MoshiStream, MoshiEvent, MoshiCommand,
    MoshiMsgType, MoshiControl, MoshiOpusEncoder, MoshiOpusDecoder,
};
#[cfg(feature = "moshi")]
pub use moshi_server::{
    MoshiServer, MoshiServerBuilder, MoshiMetadata, MoshiTtsConfig, BuildInfo,
    MOSHI_PROTOCOL_VERSION, MOSHI_MODEL_VERSION, MOSHI_SAMPLE_RATE, MOSHI_FRAME_RATE,
};
#[cfg(all(feature = "moshi", feature = "whisper"))]
pub use moshi_server::{start_moshi_server, start_moshi_server_random_port};

// Local realtime exports
#[cfg(feature = "whisper")]
pub use local_realtime::{LocalRealtimeConfig, LocalRealtimePipeline};
