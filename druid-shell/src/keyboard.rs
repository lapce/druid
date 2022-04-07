// Copyright 2020 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Keyboard types.

// This is a reasonable lint, but we keep signatures in sync with the
// bitflags implementation of the inner Modifiers type.
#![allow(clippy::trivially_copy_pass_by_ref)]

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not};

use glutin::keyboard::{KeyCode, NativeKeyCode};
pub use keyboard_types::{Code, KeyState, Location};

/// The meaning (mapped value) of a keypress.
pub type KbKey = keyboard_types::Key;

/// Information about a keyboard event.
///
/// Note that this type is similar to [`KeyboardEvent`] in keyboard-types,
/// but has a few small differences for convenience. It is missing the `state`
/// field because that is already implicit in the event.
///
/// [`KeyboardEvent`]: keyboard_types::KeyboardEvent
#[non_exhaustive]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct KeyEvent {
    /// Whether the key is pressed or released.
    pub state: KeyState,
    /// Logical key value.
    pub key: KbKey,
    /// Physical key position.
    pub code: KeyCode,
    /// Location for keys with multiple instances on common keyboards.
    pub location: Location,
    /// Flags for pressed modifier keys.
    pub mods: Modifiers,
    /// True if the key is currently auto-repeated.
    pub repeat: bool,
    /// Events with this flag should be ignored in a text editor
    /// and instead composition events should be used.
    pub is_composing: bool,
}

impl Default for KeyEvent {
    fn default() -> Self {
        Self {
            code: KeyCode::Unidentified(NativeKeyCode::Unidentified),
            state: KeyState::default(),
            key: KbKey::default(),
            location: Location::default(),
            mods: Modifiers::default(),
            repeat: false,
            is_composing: false,
        }
    }
}

/// The modifiers.
///
/// This type is a thin wrappers around [`keyboard_types::Modifiers`],
/// mostly for the convenience methods. If those get upstreamed, it
/// will simply become that type.
///
/// [`keyboard_types::Modifiers`]: keyboard_types::Modifiers
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Modifiers(keyboard_types::Modifiers);

/// A convenience trait for creating Key objects.
///
/// This trait is implemented by [`KbKey`] itself and also strings, which are
/// converted into the `Character` variant. It is defined this way and not
/// using the standard `Into` mechanism because `KbKey` is a type in an external
/// crate.
///
/// [`KbKey`]: KbKey
pub trait IntoKey {
    fn into_key(self) -> KbKey;
}

impl KeyEvent {
    #[doc(hidden)]
    /// Create a key event for testing purposes.
    pub fn for_test(mods: impl Into<Modifiers>, key: impl IntoKey) -> KeyEvent {
        let mods = mods.into();
        let key = key.into_key();
        KeyEvent {
            key,
            code: KeyCode::Unidentified(NativeKeyCode::Unidentified),
            location: Location::Standard,
            state: KeyState::Down,
            mods,
            is_composing: false,
            repeat: false,
        }
    }
}

impl Modifiers {
    pub const ALT: Modifiers = Modifiers(keyboard_types::Modifiers::ALT);
    pub const ALT_GRAPH: Modifiers = Modifiers(keyboard_types::Modifiers::ALT_GRAPH);
    pub const CAPS_LOCK: Modifiers = Modifiers(keyboard_types::Modifiers::CAPS_LOCK);
    pub const CONTROL: Modifiers = Modifiers(keyboard_types::Modifiers::CONTROL);
    pub const FN: Modifiers = Modifiers(keyboard_types::Modifiers::FN);
    pub const FN_LOCK: Modifiers = Modifiers(keyboard_types::Modifiers::FN_LOCK);
    pub const META: Modifiers = Modifiers(keyboard_types::Modifiers::META);
    pub const NUM_LOCK: Modifiers = Modifiers(keyboard_types::Modifiers::NUM_LOCK);
    pub const SCROLL_LOCK: Modifiers = Modifiers(keyboard_types::Modifiers::SCROLL_LOCK);
    pub const SHIFT: Modifiers = Modifiers(keyboard_types::Modifiers::SHIFT);
    pub const SYMBOL: Modifiers = Modifiers(keyboard_types::Modifiers::SYMBOL);
    pub const SYMBOL_LOCK: Modifiers = Modifiers(keyboard_types::Modifiers::SYMBOL_LOCK);
    pub const HYPER: Modifiers = Modifiers(keyboard_types::Modifiers::HYPER);
    pub const SUPER: Modifiers = Modifiers(keyboard_types::Modifiers::SUPER);

    /// Get the inner value.
    ///
    /// Note that this function might go away if our changes are upstreamed.
    pub fn raw(&self) -> keyboard_types::Modifiers {
        self.0
    }

    /// Determine whether Shift is set.
    pub fn shift(&self) -> bool {
        self.contains(Modifiers::SHIFT)
    }

    /// Determine whether Ctrl is set.
    pub fn ctrl(&self) -> bool {
        self.contains(Modifiers::CONTROL)
    }

    /// Determine whether Alt is set.
    pub fn alt(&self) -> bool {
        self.contains(Modifiers::ALT)
    }

    /// Determine whether Meta is set.
    pub fn meta(&self) -> bool {
        self.contains(Modifiers::META)
    }

    /// Returns an empty set of modifiers.
    pub fn empty() -> Modifiers {
        Default::default()
    }

    /// Returns `true` if no modifiers are set.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` if all the modifiers in `other` are set.
    pub fn contains(&self, other: Modifiers) -> bool {
        self.0.contains(other.0)
    }

    /// Inserts or removes the specified modifiers depending on the passed value.
    pub fn set(&mut self, other: Modifiers, value: bool) {
        self.0.set(other.0, value)
    }
}

impl BitAnd for Modifiers {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self {
        Modifiers(self.0 & rhs.0)
    }
}

impl BitAndAssign for Modifiers {
    // rhs is the "right-hand side" of the expression `a &= b`
    fn bitand_assign(&mut self, rhs: Self) {
        *self = Modifiers(self.0 & rhs.0)
    }
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Modifiers(self.0 | rhs.0)
    }
}

impl BitOrAssign for Modifiers {
    // rhs is the "right-hand side" of the expression `a &= b`
    fn bitor_assign(&mut self, rhs: Self) {
        *self = Modifiers(self.0 | rhs.0)
    }
}

impl BitXor for Modifiers {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self {
        Modifiers(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for Modifiers {
    // rhs is the "right-hand side" of the expression `a &= b`
    fn bitxor_assign(&mut self, rhs: Self) {
        *self = Modifiers(self.0 ^ rhs.0)
    }
}

impl Not for Modifiers {
    type Output = Self;

    fn not(self) -> Self {
        Modifiers(!self.0)
    }
}

impl IntoKey for KbKey {
    fn into_key(self) -> KbKey {
        self
    }
}

impl IntoKey for &str {
    fn into_key(self) -> KbKey {
        KbKey::Character(self.into())
    }
}

pub fn glutin_key(input: glutin::keyboard::Key<'static>) -> KbKey {
    match input {
        glutin::keyboard::Key::Character(c) => KbKey::Character(c.to_string()),
        glutin::keyboard::Key::Unidentified(_) => KbKey::Unidentified,
        glutin::keyboard::Key::Dead(_) => KbKey::Dead,
        glutin::keyboard::Key::Alt => KbKey::Alt,
        glutin::keyboard::Key::AltGraph => KbKey::AltGraph,
        glutin::keyboard::Key::CapsLock => KbKey::CapsLock,
        glutin::keyboard::Key::Control => KbKey::Control,
        glutin::keyboard::Key::Fn => KbKey::Fn,
        glutin::keyboard::Key::FnLock => KbKey::FnLock,
        glutin::keyboard::Key::NumLock => KbKey::NumLock,
        glutin::keyboard::Key::ScrollLock => KbKey::ScrollLock,
        glutin::keyboard::Key::Shift => KbKey::Shift,
        glutin::keyboard::Key::Symbol => KbKey::Symbol,
        glutin::keyboard::Key::SymbolLock => KbKey::SymbolLock,
        glutin::keyboard::Key::Hyper => KbKey::Hyper,
        glutin::keyboard::Key::Super => KbKey::Super,
        glutin::keyboard::Key::Enter => KbKey::Enter,
        glutin::keyboard::Key::Tab => KbKey::Tab,
        glutin::keyboard::Key::Space => KbKey::Character(" ".to_string()),
        glutin::keyboard::Key::ArrowDown => KbKey::ArrowDown,
        glutin::keyboard::Key::ArrowLeft => KbKey::ArrowLeft,
        glutin::keyboard::Key::ArrowRight => KbKey::ArrowRight,
        glutin::keyboard::Key::ArrowUp => KbKey::ArrowUp,
        glutin::keyboard::Key::End => KbKey::End,
        glutin::keyboard::Key::Home => KbKey::Home,
        glutin::keyboard::Key::PageDown => KbKey::PageDown,
        glutin::keyboard::Key::PageUp => KbKey::PageUp,
        glutin::keyboard::Key::Backspace => KbKey::Backspace,
        glutin::keyboard::Key::Clear => KbKey::Clear,
        glutin::keyboard::Key::Copy => KbKey::Copy,
        glutin::keyboard::Key::CrSel => KbKey::CrSel,
        glutin::keyboard::Key::Cut => KbKey::Cut,
        glutin::keyboard::Key::Delete => KbKey::Delete,
        glutin::keyboard::Key::EraseEof => KbKey::EraseEof,
        glutin::keyboard::Key::ExSel => KbKey::ExSel,
        glutin::keyboard::Key::Insert => KbKey::Insert,
        glutin::keyboard::Key::Paste => KbKey::Paste,
        glutin::keyboard::Key::Redo => KbKey::Redo,
        glutin::keyboard::Key::Undo => KbKey::Undo,
        glutin::keyboard::Key::Accept => KbKey::Accept,
        glutin::keyboard::Key::Again => KbKey::Again,
        glutin::keyboard::Key::Attn => KbKey::Attn,
        glutin::keyboard::Key::Cancel => KbKey::Cancel,
        glutin::keyboard::Key::ContextMenu => KbKey::ContextMenu,
        glutin::keyboard::Key::Escape => KbKey::Escape,
        glutin::keyboard::Key::Execute => KbKey::Execute,
        glutin::keyboard::Key::Find => KbKey::Find,
        glutin::keyboard::Key::Help => KbKey::Help,
        glutin::keyboard::Key::Pause => KbKey::Pause,
        glutin::keyboard::Key::Play => KbKey::Play,
        glutin::keyboard::Key::Props => KbKey::Props,
        glutin::keyboard::Key::Select => KbKey::Select,
        glutin::keyboard::Key::ZoomIn => KbKey::ZoomIn,
        glutin::keyboard::Key::ZoomOut => KbKey::ZoomOut,
        glutin::keyboard::Key::BrightnessDown => KbKey::BrightnessDown,
        glutin::keyboard::Key::BrightnessUp => KbKey::BrightnessUp,
        glutin::keyboard::Key::Eject => KbKey::Eject,
        glutin::keyboard::Key::LogOff => KbKey::LogOff,
        glutin::keyboard::Key::Power => KbKey::Power,
        glutin::keyboard::Key::PowerOff => KbKey::PowerOff,
        glutin::keyboard::Key::PrintScreen => KbKey::PrintScreen,
        glutin::keyboard::Key::Hibernate => KbKey::Hibernate,
        glutin::keyboard::Key::Standby => KbKey::Standby,
        glutin::keyboard::Key::WakeUp => KbKey::WakeUp,
        glutin::keyboard::Key::AllCandidates => KbKey::AllCandidates,
        glutin::keyboard::Key::Alphanumeric => KbKey::Alphanumeric,
        glutin::keyboard::Key::CodeInput => KbKey::CodeInput,
        glutin::keyboard::Key::Compose => KbKey::Compose,
        glutin::keyboard::Key::Convert => KbKey::Convert,
        glutin::keyboard::Key::FinalMode => KbKey::FinalMode,
        glutin::keyboard::Key::GroupFirst => KbKey::GroupFirst,
        glutin::keyboard::Key::GroupLast => KbKey::GroupLast,
        glutin::keyboard::Key::GroupNext => KbKey::GroupNext,
        glutin::keyboard::Key::GroupPrevious => KbKey::GroupPrevious,
        glutin::keyboard::Key::ModeChange => KbKey::ModeChange,
        glutin::keyboard::Key::NextCandidate => KbKey::NextCandidate,
        glutin::keyboard::Key::NonConvert => KbKey::NonConvert,
        glutin::keyboard::Key::PreviousCandidate => KbKey::PreviousCandidate,
        glutin::keyboard::Key::Process => KbKey::Process,
        glutin::keyboard::Key::SingleCandidate => KbKey::SingleCandidate,
        glutin::keyboard::Key::HangulMode => KbKey::HangulMode,
        glutin::keyboard::Key::HanjaMode => KbKey::HanjaMode,
        glutin::keyboard::Key::JunjaMode => KbKey::JunjaMode,
        glutin::keyboard::Key::Eisu => KbKey::Eisu,
        glutin::keyboard::Key::Hankaku => KbKey::Hankaku,
        glutin::keyboard::Key::Hiragana => KbKey::Hiragana,
        glutin::keyboard::Key::HiraganaKatakana => KbKey::HiraganaKatakana,
        glutin::keyboard::Key::KanaMode => KbKey::KanaMode,
        glutin::keyboard::Key::KanjiMode => KbKey::KanjiMode,
        glutin::keyboard::Key::Katakana => KbKey::Katakana,
        glutin::keyboard::Key::Romaji => KbKey::Romaji,
        glutin::keyboard::Key::Zenkaku => KbKey::Zenkaku,
        glutin::keyboard::Key::ZenkakuHankaku => KbKey::ZenkakuHankaku,
        glutin::keyboard::Key::Soft1 => KbKey::Soft1,
        glutin::keyboard::Key::Soft2 => KbKey::Soft2,
        glutin::keyboard::Key::Soft3 => KbKey::Soft3,
        glutin::keyboard::Key::Soft4 => KbKey::Soft4,
        glutin::keyboard::Key::ChannelDown => KbKey::ChannelDown,
        glutin::keyboard::Key::ChannelUp => KbKey::ChannelUp,
        glutin::keyboard::Key::Close => KbKey::Close,
        glutin::keyboard::Key::MailForward => KbKey::MailForward,
        glutin::keyboard::Key::MailReply => KbKey::MailReply,
        glutin::keyboard::Key::MailSend => KbKey::MailSend,
        glutin::keyboard::Key::MediaClose => KbKey::MediaClose,
        glutin::keyboard::Key::MediaFastForward => KbKey::MediaFastForward,
        glutin::keyboard::Key::MediaPause => KbKey::MediaPause,
        glutin::keyboard::Key::MediaPlay => KbKey::MediaPlay,
        glutin::keyboard::Key::MediaPlayPause => KbKey::MediaPlayPause,
        glutin::keyboard::Key::MediaRecord => KbKey::MediaRecord,
        glutin::keyboard::Key::MediaRewind => KbKey::MediaRewind,
        glutin::keyboard::Key::MediaStop => KbKey::MediaStop,
        glutin::keyboard::Key::MediaTrackNext => KbKey::MediaTrackNext,
        glutin::keyboard::Key::MediaTrackPrevious => KbKey::MediaTrackPrevious,
        glutin::keyboard::Key::New => KbKey::New,
        glutin::keyboard::Key::Open => KbKey::Open,
        glutin::keyboard::Key::Print => KbKey::Print,
        glutin::keyboard::Key::Save => KbKey::Save,
        glutin::keyboard::Key::SpellCheck => KbKey::SpellCheck,
        glutin::keyboard::Key::Key11 => KbKey::Key11,
        glutin::keyboard::Key::Key12 => KbKey::Key12,
        glutin::keyboard::Key::AudioBalanceLeft => KbKey::AudioBalanceLeft,
        glutin::keyboard::Key::AudioBalanceRight => KbKey::AudioBalanceRight,
        glutin::keyboard::Key::AudioBassBoostDown => KbKey::AudioBassBoostDown,
        glutin::keyboard::Key::AudioBassBoostToggle => KbKey::AudioBassBoostToggle,
        glutin::keyboard::Key::AudioBassBoostUp => KbKey::AudioBassBoostUp,
        glutin::keyboard::Key::AudioFaderFront => KbKey::AudioFaderFront,
        glutin::keyboard::Key::AudioFaderRear => KbKey::AudioFaderRear,
        glutin::keyboard::Key::AudioSurroundModeNext => KbKey::AudioSurroundModeNext,
        glutin::keyboard::Key::AudioTrebleDown => KbKey::AudioTrebleDown,
        glutin::keyboard::Key::AudioTrebleUp => KbKey::AudioTrebleUp,
        glutin::keyboard::Key::AudioVolumeDown => KbKey::AudioVolumeDown,
        glutin::keyboard::Key::AudioVolumeUp => KbKey::AudioVolumeUp,
        glutin::keyboard::Key::AudioVolumeMute => KbKey::AudioVolumeMute,
        glutin::keyboard::Key::MicrophoneToggle => KbKey::MicrophoneToggle,
        glutin::keyboard::Key::MicrophoneVolumeDown => KbKey::MicrophoneVolumeDown,
        glutin::keyboard::Key::MicrophoneVolumeUp => KbKey::MicrophoneVolumeUp,
        glutin::keyboard::Key::MicrophoneVolumeMute => KbKey::MicrophoneVolumeMute,
        glutin::keyboard::Key::SpeechCorrectionList => KbKey::SpeechCorrectionList,
        glutin::keyboard::Key::SpeechInputToggle => KbKey::SpeechInputToggle,
        glutin::keyboard::Key::LaunchApplication1 => KbKey::LaunchApplication1,
        glutin::keyboard::Key::LaunchApplication2 => KbKey::LaunchApplication2,
        glutin::keyboard::Key::LaunchCalendar => KbKey::LaunchCalendar,
        glutin::keyboard::Key::LaunchContacts => KbKey::LaunchContacts,
        glutin::keyboard::Key::LaunchMail => KbKey::LaunchMail,
        glutin::keyboard::Key::LaunchMediaPlayer => KbKey::LaunchMediaPlayer,
        glutin::keyboard::Key::LaunchMusicPlayer => KbKey::LaunchMusicPlayer,
        glutin::keyboard::Key::LaunchPhone => KbKey::LaunchPhone,
        glutin::keyboard::Key::LaunchScreenSaver => KbKey::LaunchScreenSaver,
        glutin::keyboard::Key::LaunchSpreadsheet => KbKey::LaunchSpreadsheet,
        glutin::keyboard::Key::LaunchWebBrowser => KbKey::LaunchWebBrowser,
        glutin::keyboard::Key::LaunchWebCam => KbKey::LaunchWebCam,
        glutin::keyboard::Key::LaunchWordProcessor => KbKey::LaunchWordProcessor,
        glutin::keyboard::Key::BrowserBack => KbKey::BrowserBack,
        glutin::keyboard::Key::BrowserFavorites => KbKey::BrowserFavorites,
        glutin::keyboard::Key::BrowserForward => KbKey::BrowserForward,
        glutin::keyboard::Key::BrowserHome => KbKey::BrowserHome,
        glutin::keyboard::Key::BrowserRefresh => KbKey::BrowserRefresh,
        glutin::keyboard::Key::BrowserSearch => KbKey::BrowserSearch,
        glutin::keyboard::Key::BrowserStop => KbKey::BrowserStop,
        glutin::keyboard::Key::AppSwitch => KbKey::AppSwitch,
        glutin::keyboard::Key::Call => KbKey::Call,
        glutin::keyboard::Key::Camera => KbKey::Camera,
        glutin::keyboard::Key::CameraFocus => KbKey::CameraFocus,
        glutin::keyboard::Key::EndCall => KbKey::EndCall,
        glutin::keyboard::Key::GoBack => KbKey::GoBack,
        glutin::keyboard::Key::GoHome => KbKey::GoHome,
        glutin::keyboard::Key::HeadsetHook => KbKey::HeadsetHook,
        glutin::keyboard::Key::LastNumberRedial => KbKey::LastNumberRedial,
        glutin::keyboard::Key::Notification => KbKey::Notification,
        glutin::keyboard::Key::MannerMode => KbKey::MannerMode,
        glutin::keyboard::Key::VoiceDial => KbKey::VoiceDial,
        glutin::keyboard::Key::TV => KbKey::TV,
        glutin::keyboard::Key::TV3DMode => KbKey::TV3DMode,
        glutin::keyboard::Key::TVAntennaCable => KbKey::TVAntennaCable,
        glutin::keyboard::Key::TVAudioDescription => KbKey::TVAudioDescription,
        glutin::keyboard::Key::TVAudioDescriptionMixDown => KbKey::TVAudioDescriptionMixDown,
        glutin::keyboard::Key::TVAudioDescriptionMixUp => KbKey::TVAudioDescriptionMixUp,
        glutin::keyboard::Key::TVContentsMenu => KbKey::TVContentsMenu,
        glutin::keyboard::Key::TVDataService => KbKey::TVDataService,
        glutin::keyboard::Key::TVInput => KbKey::TVInput,
        glutin::keyboard::Key::TVInputComponent1 => KbKey::TVInputComponent1,
        glutin::keyboard::Key::TVInputComponent2 => KbKey::TVInputComponent2,
        glutin::keyboard::Key::TVInputComposite1 => KbKey::TVInputComposite1,
        glutin::keyboard::Key::TVInputComposite2 => KbKey::TVInputComposite2,
        glutin::keyboard::Key::TVInputHDMI1 => KbKey::TVInputHDMI1,
        glutin::keyboard::Key::TVInputHDMI2 => KbKey::TVInputHDMI2,
        glutin::keyboard::Key::TVInputHDMI3 => KbKey::TVInputHDMI3,
        glutin::keyboard::Key::TVInputHDMI4 => KbKey::TVInputHDMI4,
        glutin::keyboard::Key::TVInputVGA1 => KbKey::TVInputVGA1,
        glutin::keyboard::Key::TVMediaContext => KbKey::TVMediaContext,
        glutin::keyboard::Key::TVNetwork => KbKey::TVNetwork,
        glutin::keyboard::Key::TVNumberEntry => KbKey::TVNumberEntry,
        glutin::keyboard::Key::TVPower => KbKey::TVPower,
        glutin::keyboard::Key::TVRadioService => KbKey::TVRadioService,
        glutin::keyboard::Key::TVSatellite => KbKey::TVSatellite,
        glutin::keyboard::Key::TVSatelliteBS => KbKey::TVSatelliteBS,
        glutin::keyboard::Key::TVSatelliteCS => KbKey::TVSatelliteCS,
        glutin::keyboard::Key::TVSatelliteToggle => KbKey::TVSatelliteToggle,
        glutin::keyboard::Key::TVTerrestrialAnalog => KbKey::TVTerrestrialAnalog,
        glutin::keyboard::Key::TVTerrestrialDigital => KbKey::TVTerrestrialDigital,
        glutin::keyboard::Key::TVTimer => KbKey::TVTimer,
        glutin::keyboard::Key::AVRInput => KbKey::AVRInput,
        glutin::keyboard::Key::AVRPower => KbKey::AVRPower,
        glutin::keyboard::Key::ColorF0Red => KbKey::ColorF0Red,
        glutin::keyboard::Key::ColorF1Green => KbKey::ColorF1Green,
        glutin::keyboard::Key::ColorF2Yellow => KbKey::ColorF2Yellow,
        glutin::keyboard::Key::ColorF3Blue => KbKey::ColorF3Blue,
        glutin::keyboard::Key::ColorF4Grey => KbKey::ColorF4Grey,
        glutin::keyboard::Key::ColorF5Brown => KbKey::ColorF5Brown,
        glutin::keyboard::Key::ClosedCaptionToggle => KbKey::ClosedCaptionToggle,
        glutin::keyboard::Key::Dimmer => KbKey::Dimmer,
        glutin::keyboard::Key::DisplaySwap => KbKey::DisplaySwap,
        glutin::keyboard::Key::DVR => KbKey::DVR,
        glutin::keyboard::Key::Exit => KbKey::Exit,
        glutin::keyboard::Key::FavoriteClear0 => KbKey::FavoriteClear0,
        glutin::keyboard::Key::FavoriteClear1 => KbKey::FavoriteClear1,
        glutin::keyboard::Key::FavoriteClear2 => KbKey::FavoriteClear2,
        glutin::keyboard::Key::FavoriteClear3 => KbKey::FavoriteClear3,
        glutin::keyboard::Key::FavoriteRecall0 => KbKey::FavoriteRecall0,
        glutin::keyboard::Key::FavoriteRecall1 => KbKey::FavoriteRecall1,
        glutin::keyboard::Key::FavoriteRecall2 => KbKey::FavoriteRecall2,
        glutin::keyboard::Key::FavoriteRecall3 => KbKey::FavoriteRecall3,
        glutin::keyboard::Key::FavoriteStore0 => KbKey::FavoriteStore0,
        glutin::keyboard::Key::FavoriteStore1 => KbKey::FavoriteStore1,
        glutin::keyboard::Key::FavoriteStore2 => KbKey::FavoriteStore2,
        glutin::keyboard::Key::FavoriteStore3 => KbKey::FavoriteStore3,
        glutin::keyboard::Key::Guide => KbKey::Guide,
        glutin::keyboard::Key::GuideNextDay => KbKey::GuideNextDay,
        glutin::keyboard::Key::GuidePreviousDay => KbKey::GuidePreviousDay,
        glutin::keyboard::Key::Info => KbKey::Info,
        glutin::keyboard::Key::InstantReplay => KbKey::InstantReplay,
        glutin::keyboard::Key::Link => KbKey::Link,
        glutin::keyboard::Key::ListProgram => KbKey::ListProgram,
        glutin::keyboard::Key::LiveContent => KbKey::LiveContent,
        glutin::keyboard::Key::Lock => KbKey::Lock,
        glutin::keyboard::Key::MediaApps => KbKey::MediaApps,
        glutin::keyboard::Key::MediaAudioTrack => KbKey::MediaAudioTrack,
        glutin::keyboard::Key::MediaLast => KbKey::MediaLast,
        glutin::keyboard::Key::MediaSkipBackward => KbKey::MediaSkipBackward,
        glutin::keyboard::Key::MediaSkipForward => KbKey::MediaSkipForward,
        glutin::keyboard::Key::MediaStepBackward => KbKey::MediaStepBackward,
        glutin::keyboard::Key::MediaStepForward => KbKey::MediaStepForward,
        glutin::keyboard::Key::MediaTopMenu => KbKey::MediaTopMenu,
        glutin::keyboard::Key::NavigateIn => KbKey::NavigateIn,
        glutin::keyboard::Key::NavigateNext => KbKey::NavigateNext,
        glutin::keyboard::Key::NavigateOut => KbKey::NavigateOut,
        glutin::keyboard::Key::NavigatePrevious => KbKey::NavigatePrevious,
        glutin::keyboard::Key::NextFavoriteChannel => KbKey::NextFavoriteChannel,
        glutin::keyboard::Key::NextUserProfile => KbKey::NextUserProfile,
        glutin::keyboard::Key::OnDemand => KbKey::OnDemand,
        glutin::keyboard::Key::Pairing => KbKey::Pairing,
        glutin::keyboard::Key::PinPDown => KbKey::PinPDown,
        glutin::keyboard::Key::PinPMove => KbKey::PinPMove,
        glutin::keyboard::Key::PinPToggle => KbKey::PinPToggle,
        glutin::keyboard::Key::PinPUp => KbKey::PinPUp,
        glutin::keyboard::Key::PlaySpeedDown => KbKey::PlaySpeedDown,
        glutin::keyboard::Key::PlaySpeedReset => KbKey::PlaySpeedReset,
        glutin::keyboard::Key::PlaySpeedUp => KbKey::PlaySpeedUp,
        glutin::keyboard::Key::RandomToggle => KbKey::RandomToggle,
        glutin::keyboard::Key::RcLowBattery => KbKey::RcLowBattery,
        glutin::keyboard::Key::RecordSpeedNext => KbKey::RecordSpeedNext,
        glutin::keyboard::Key::RfBypass => KbKey::RfBypass,
        glutin::keyboard::Key::ScanChannelsToggle => KbKey::ScanChannelsToggle,
        glutin::keyboard::Key::ScreenModeNext => KbKey::ScreenModeNext,
        glutin::keyboard::Key::Settings => KbKey::Settings,
        glutin::keyboard::Key::SplitScreenToggle => KbKey::SplitScreenToggle,
        glutin::keyboard::Key::STBInput => KbKey::STBInput,
        glutin::keyboard::Key::STBPower => KbKey::STBPower,
        glutin::keyboard::Key::Subtitle => KbKey::Subtitle,
        glutin::keyboard::Key::Teletext => KbKey::Teletext,
        glutin::keyboard::Key::VideoModeNext => KbKey::VideoModeNext,
        glutin::keyboard::Key::Wink => KbKey::Wink,
        glutin::keyboard::Key::ZoomToggle => KbKey::ZoomToggle,
        glutin::keyboard::Key::F1 => KbKey::F1,
        glutin::keyboard::Key::F2 => KbKey::F2,
        glutin::keyboard::Key::F3 => KbKey::F3,
        glutin::keyboard::Key::F4 => KbKey::F4,
        glutin::keyboard::Key::F5 => KbKey::F5,
        glutin::keyboard::Key::F6 => KbKey::F6,
        glutin::keyboard::Key::F7 => KbKey::F7,
        glutin::keyboard::Key::F8 => KbKey::F8,
        glutin::keyboard::Key::F9 => KbKey::F9,
        glutin::keyboard::Key::F10 => KbKey::F10,
        glutin::keyboard::Key::F11 => KbKey::F11,
        glutin::keyboard::Key::F12 => KbKey::F12,
        glutin::keyboard::Key::F13 => KbKey::Unidentified,
        glutin::keyboard::Key::F14 => KbKey::Unidentified,
        glutin::keyboard::Key::F15 => KbKey::Unidentified,
        glutin::keyboard::Key::F16 => KbKey::Unidentified,
        glutin::keyboard::Key::F17 => KbKey::Unidentified,
        glutin::keyboard::Key::F18 => KbKey::Unidentified,
        glutin::keyboard::Key::F19 => KbKey::Unidentified,
        glutin::keyboard::Key::F20 => KbKey::Unidentified,
        glutin::keyboard::Key::F21 => KbKey::Unidentified,
        glutin::keyboard::Key::F22 => KbKey::Unidentified,
        glutin::keyboard::Key::F23 => KbKey::Unidentified,
        glutin::keyboard::Key::F24 => KbKey::Unidentified,
        glutin::keyboard::Key::F25 => KbKey::Unidentified,
        glutin::keyboard::Key::F26 => KbKey::Unidentified,
        glutin::keyboard::Key::F27 => KbKey::Unidentified,
        glutin::keyboard::Key::F28 => KbKey::Unidentified,
        glutin::keyboard::Key::F29 => KbKey::Unidentified,
        glutin::keyboard::Key::F30 => KbKey::Unidentified,
        glutin::keyboard::Key::F31 => KbKey::Unidentified,
        glutin::keyboard::Key::F32 => KbKey::Unidentified,
        glutin::keyboard::Key::F33 => KbKey::Unidentified,
        glutin::keyboard::Key::F34 => KbKey::Unidentified,
        glutin::keyboard::Key::F35 => KbKey::Unidentified,
        _ => KbKey::Unidentified,
    }
}
