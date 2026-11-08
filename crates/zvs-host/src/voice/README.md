# voice

Voice input for the chat composer: record from a microphone, transcribe locally with
whisper.cpp, insert the text. Nothing leaves the machine. The engine and the speech models are
installed through the download queue like any other model.

## Files

| File           | Contents                                                                                    |
| -------------- | ------------------------------------------------------------------------------------------- |
| `mod.rs`       | `VoiceService`: devices, model list and selection, install, start / stop / cancel recording |
| `catalogue.rs` | The whisper.cpp engine build and three speech models; `recommend`; the catalogue provider   |
| `whisper.rs`   | `WhisperCli` (the `SpeechRecognizer`), WAV encoding, transcript cleanup, finding the CLI    |

## Models (`catalogue.rs`)

| Model                           | File                           | Suggested for |
| ------------------------------- | ------------------------------ | ------------- |
| `whisper-base` (q8_0)           | `ggml-base-q8_0.bin`           | low tier      |
| `whisper-small` (q8_0)          | `ggml-small-q8_0.bin`          | medium tier   |
| `whisper-large-v3-turbo` (q5_0) | `ggml-large-v3-turbo-q5_0.bin` | high tier     |

Models come from `huggingface.co/ggerganov/whisper.cpp`; the engine is the pinned
`whisper-blas-bin-x64.zip` release of whisper.cpp. `recommend(device)` picks by the device tier
(`../platform/README.md`) and returns the reason shown to the user. Speech is offered only on
Windows x64, the platform the engine build targets (`speech_supported`).

## Pipeline: one dictation

1. **Record.** `start_recording` opens the selected capture device through the `AudioInput`
   port (WASAPI in the shell) as 16-bit samples at 16 kHz. Recording is capped at 10 minutes
   (`MAX_RECORDING_SAMPLES`); a second start fails with `ALREADY_RECORDING`.
2. **Stop.** `stop_recording` takes the samples; under a quarter of a second
   (`MIN_SPEECH_SAMPLES`) returns an empty transcript without running the engine.
3. **Transcribe.** The samples are written as a WAV file (`wav_bytes`) and passed to
   `whisper-cli` (`-m model -f file -l auto -t threads -nt -np -otxt -of out`): language
   auto-detected, no timestamps, no progress, text output, up to 8 threads.
4. **Clean.** `clean_transcript` removes bracketed non-speech annotations such as
   `[музыка]` or `[BLANK_AUDIO]` and collapses whitespace.

`cancel_recording` discards the samples. `find_cli` locates `whisper-cli` inside the unpacked
engine archive (searching up to 4 levels deep), so the archive layout can change between
releases.
