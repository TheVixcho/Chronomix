#pragma once

#include <juce_audio_formats/juce_audio_formats.h>
#include <juce_core/juce_core.h>
#include <juce_gui_basics/juce_gui_basics.h>
#include "chronomix_ffi.h"
#include <memory>
#include <vector>

class ChronomixBridge
{
public:
    ChronomixBridge()
    {
        handle = chronomix_create();
        formatManager.registerBasicFormats();
    }

    ~ChronomixBridge()
    {
        if (handle != nullptr)
        {
            chronomix_destroy(handle);
            handle = nullptr;
        }
    }

    ChronomixBridge(const ChronomixBridge&) = delete;
    ChronomixBridge& operator=(const ChronomixBridge&) = delete;

    ChronomixAppHandle* raw() const noexcept { return handle; }

    static juce::String takeString(char* ptr)
    {
        if (ptr == nullptr)
            return {};
        juce::String s = juce::String::fromUTF8(ptr);
        chronomix_free_string(ptr);
        return s;
    }

    static juce::var takeJson(char* ptr)
    {
        auto text = takeString(ptr);
        if (text.isEmpty())
            return {};
        return juce::JSON::parse(text);
    }

    void tick() { chronomix_tick(handle); }

    juce::var getPaths() const { return takeJson(chronomix_get_paths_json(handle)); }
    juce::var getSettings() const { return takeJson(chronomix_get_settings_json(handle)); }
    bool updateSettings(const juce::var& v)
    {
        auto s = juce::JSON::toString(v, true);
        return chronomix_update_settings_json(handle, s.toRawUTF8()) != 0;
    }

    juce::String takeLastError() { return takeString(chronomix_take_last_error(handle)); }
    juce::String getStatusText() const { return takeString(chronomix_get_status_text(handle)); }
    juce::String getWineWarning() const { return takeString(chronomix_get_wine_warning(handle)); }

    juce::var listDevices() const { return takeJson(chronomix_list_devices_json(handle)); }
    void refreshDevices() { chronomix_refresh_devices(handle); }

    bool setOutputDevice(const juce::String& outputId, uint32_t sr, uint32_t block)
    {
        return chronomix_set_output_device(handle, outputId.toRawUTF8(), sr, block) != 0;
    }

    bool setChannelInput(uint32_t ch, const juce::String& deviceId, const juce::String& deviceLabel, uint8_t mode)
    {
        return chronomix_set_channel_input(handle, ch, deviceId.toRawUTF8(), deviceLabel.toRawUTF8(), mode) != 0;
    }

    bool startAudio(const juce::String& outputId, uint32_t sr, uint32_t block)
    {
        return chronomix_start_audio(handle, outputId.toRawUTF8(), sr, block) != 0;
    }

    void stopAudio() { chronomix_stop_audio(handle); }

    bool decodeAudioFile(const juce::File& file,
                         std::vector<float>& outStereo,
                         uint32_t& outFrames,
                         uint32_t& outSampleRate,
                         juce::String& outError)
    {
        outStereo.clear();
        outFrames = 0;
        outSampleRate = 48000;
        if (!file.existsAsFile())
        {
            outError = "Archivo no encontrado: " + file.getFullPathName();
            return false;
        }
        std::unique_ptr<juce::AudioFormatReader> reader(formatManager.createReaderFor(file));
        if (reader == nullptr)
        {
            outError = "Formato de audio no soportado por el decodificador nativo.";
            return false;
        }
        const auto total = static_cast<int64_t>(reader->lengthInSamples);
        if (total <= 0)
        {
            outError = "El archivo de audio está vacío.";
            return false;
        }
        const int maxSamples = static_cast<int>(juce::jmin<int64_t>(total, 48000LL * 60LL * 20LL));
        const int numCh = juce::jlimit(1, 2, static_cast<int>(reader->numChannels));
        juce::AudioBuffer<float> buf(numCh, maxSamples);
        if (!reader->read(&buf, 0, maxSamples, 0, true, numCh > 1))
        {
            outError = "Error leyendo muestras de audio.";
            return false;
        }
        outStereo.resize(static_cast<size_t>(maxSamples) * 2);
        const float* ch0 = buf.getReadPointer(0);
        const float* ch1 = (numCh > 1) ? buf.getReadPointer(1) : ch0;
        for (int i = 0; i < maxSamples; ++i)
        {
            outStereo[static_cast<size_t>(i) * 2] = ch0[i];
            outStereo[static_cast<size_t>(i) * 2 + 1] = ch1[i];
        }
        outFrames = static_cast<uint32_t>(maxSamples);
        outSampleRate = static_cast<uint32_t>(juce::jmax(8000.0, reader->sampleRate));
        return true;
    }

    bool loadDjDeckFile(uint32_t deck, const juce::File& file, juce::String& errOut)
    {
        std::vector<float> pcm;
        uint32_t frames = 0;
        uint32_t sr = 48000;
        if (decodeAudioFile(file, pcm, frames, sr, errOut))
        {
            return chronomix_dj_load_pcm(
                handle,
                deck,
                pcm.data(),
                frames,
                sr,
                file.getFileNameWithoutExtension().toRawUTF8(),
                file.getFullPathName().toRawUTF8()) != 0;
        }
        if (chronomix_dj_load_file(handle, deck, file.getFullPathName().toRawUTF8()) != 0)
        {
            errOut.clear();
            return true;
        }
        auto rustErr = takeLastError();
        if (rustErr.isNotEmpty())
            errOut = rustErr;
        return false;
    }

    bool loadMixerMediaFile(uint32_t ch, const juce::File& file, juce::String& errOut)
    {
        std::vector<float> pcm;
        uint32_t frames = 0;
        uint32_t sr = 48000;
        if (decodeAudioFile(file, pcm, frames, sr, errOut))
        {
            return chronomix_load_channel_media_pcm(
                handle,
                ch,
                pcm.data(),
                frames,
                sr,
                file.getFileNameWithoutExtension().toRawUTF8(),
                file.getFullPathName().toRawUTF8()) != 0;
        }
        if (chronomix_load_channel_media_file(handle, ch, file.getFullPathName().toRawUTF8()) != 0)
        {
            errOut.clear();
            return true;
        }
        auto rustErr = takeLastError();
        if (rustErr.isNotEmpty())
            errOut = rustErr;
        return false;
    }

    juce::String getSupportedAudioWildcard() const
    {
        return "*.wav;*.mp3;*.flac;*.ogg;*.aiff;*.aif;*.m4a";
    }

private:
    ChronomixAppHandle* handle = nullptr;
    juce::AudioFormatManager formatManager;
};
