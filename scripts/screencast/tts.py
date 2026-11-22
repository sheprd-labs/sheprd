"""Text to speech for screencasts: macOS `say`, OpenAI, or ElevenLabs.

Each provider writes one audio file per line of narration. Cloud results are
cached by provider, voice, and text, so re-rendering a feature costs nothing
until its narration changes. Keys come from the environment:
OPENAI_API_KEY, ELEVENLABS_API_KEY (and optionally OPENAI_TTS_VOICE,
ELEVENLABS_VOICE_ID).
"""

import hashlib
import json
import os
import subprocess
import urllib.request
from pathlib import Path

LANGUAGE_NAMES = {
    "en": "English",
    "fr": "French",
    "es": "Spanish",
    "de": "German",
    "pt-BR": "Brazilian Portuguese",
    "ja": "Japanese",
    "zh-CN": "Mandarin Chinese",
}

SAY_VOICES = {
    "en": "Samantha",
    "fr": "Thomas",
    "es": "Mónica",
    "de": "Anna",
    "pt-BR": "Luciana",
    "ja": "Kyoko",
    "zh-CN": "Tingting",
}

OPENAI_MODEL = "gpt-4o-mini-tts"
ELEVENLABS_MODEL = os.environ.get("ELEVENLABS_MODEL", "eleven_v4")
# Voices picked in the voice tryouts (README.md, "Voices"). River and Eric are
# premade voices every account has; the others come from the voice library.
ELEVENLABS_VOICES = {
    "damien": "kSNoLVC4Ox0vkBz1IKux",  # Damien Doré, deep narrator, fr-FR
    "eric": "cjVigY5qzO86Huf0OWal",
    "julien": "VuE88ydpXAhBTNxFI7WF",  # Julien, friendly tech explainer, fr-FR
    "river": "SAz9YHcvj6GT2YYXdXww",
}
ELEVENLABS_DEFAULT_VOICE = ELEVENLABS_VOICES["river"]
# A native voice for a language beats the voice named on the command line: the
# English voices speak French with a Canadian accent.
ELEVENLABS_NATIVE = {
    "fr": "julien",  # fallback: "damien"
}
# The language_code ElevenLabs takes (ISO 639-1).
ELEVENLABS_LANGUAGE = {"pt-BR": "pt", "zh-CN": "zh"}

PROVIDERS = ("say", "openai", "elevenlabs", "elevenlabs:VOICE")


def _post(url, headers, body):
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers=headers)
    with urllib.request.urlopen(req, timeout=120) as resp:
        return resp.read()


def _openai(text, lang):
    key = os.environ["OPENAI_API_KEY"]
    voice = os.environ.get("OPENAI_TTS_VOICE", "coral")
    body = {
        "model": OPENAI_MODEL,
        "voice": voice,
        "input": text,
        "response_format": "mp3",
        "instructions": (
            f"Speak {LANGUAGE_NAMES[lang]} natively, as a friendly, confident "
            "narrator of a short product demo. Clear, warm, unhurried."
        ),
    }
    headers = {"Authorization": f"Bearer {key}", "Content-Type": "application/json"}
    return voice, _post("https://api.openai.com/v1/audio/speech", headers, body)


def _elevenlabs_voice(provider, lang):
    native = os.environ.get(f"ELEVENLABS_VOICE_{lang.upper().replace('-', '_')}",
                            ELEVENLABS_NATIVE.get(lang))
    if native:
        return ELEVENLABS_VOICES.get(native, native)
    _, _, name = provider.partition(":")
    if name:
        return ELEVENLABS_VOICES.get(name, name)
    return os.environ.get("ELEVENLABS_VOICE_ID", ELEVENLABS_DEFAULT_VOICE)


def _elevenlabs(text, lang, voice):
    key = os.environ["ELEVENLABS_API_KEY"]
    body = {
        "text": text,
        "model_id": ELEVENLABS_MODEL,
        "language_code": ELEVENLABS_LANGUAGE.get(lang, lang),
    }
    headers = {"xi-api-key": key, "Content-Type": "application/json",
               "Accept": "audio/mpeg"}
    url = (f"https://api.elevenlabs.io/v1/text-to-speech/{voice}"
           "?output_format=mp3_44100_128")
    return _post(url, headers, body)


def available(provider):
    if provider.startswith("elevenlabs"):
        return bool(os.environ.get("ELEVENLABS_API_KEY"))
    return {"say": True, "openai": bool(os.environ.get("OPENAI_API_KEY"))}[provider]


def synthesize(provider, text, lang, path, cache):
    """Write `text` spoken in `lang` to `path` (an .aiff for say, else .mp3)."""
    if provider == "say":
        subprocess.run(["say", "-v", SAY_VOICES[lang], "-o", str(path), text],
                       check=True)
        return
    if provider == "openai":
        voice = os.environ.get("OPENAI_TTS_VOICE", "coral")
        model = OPENAI_MODEL
    else:
        voice = _elevenlabs_voice(provider, lang)
        model = ELEVENLABS_MODEL
    key = f"{model}\0{voice}\0{lang}\0{text}"
    cached = Path(cache) / f"{hashlib.sha256(key.encode()).hexdigest()[:32]}.mp3"
    if not cached.exists():
        cached.parent.mkdir(parents=True, exist_ok=True)
        if provider == "openai":
            _, audio = _openai(text, lang)
        else:
            audio = _elevenlabs(text, lang, voice)
        cached.write_bytes(audio)
    path.write_bytes(cached.read_bytes())


def extension(provider):
    return "aiff" if provider == "say" else "mp3"
