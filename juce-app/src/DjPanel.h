#pragma once

#include <juce_gui_basics/juce_gui_basics.h>
#include "FfiBridge.h"
#include "ChronomixLookAndFeel.h"
#include <array>
#include <functional>
#include <vector>

class DjWaveformView : public juce::Component
{
public:
    DjWaveformView(juce::Colour deckAccent);
    void setWaveform(const std::vector<CWaveformBin>& bins);
    void setTelemetry(const CDjDeckTelemetry& t);
    std::function<void(double)> onSeekNorm;

    void paint(juce::Graphics& g) override;
    void mouseDown(const juce::MouseEvent& e) override;
    void mouseDrag(const juce::MouseEvent& e) override;

private:
    juce::Colour accent;
    std::vector<CWaveformBin> waveform;
    CDjDeckTelemetry tel {};
    void seekAtX(int x);
};

class DjJogWheel : public juce::Component
{
public:
    DjJogWheel(juce::Colour deckAccent, juce::String deckTag);
    void setTelemetry(const CDjDeckTelemetry& t);
    std::function<void(double)> onNudgeSecs;

    void paint(juce::Graphics& g) override;
    void mouseDown(const juce::MouseEvent& e) override;
    void mouseDrag(const juce::MouseEvent& e) override;
    void mouseUp(const juce::MouseEvent& e) override;

private:
    juce::Colour accent;
    juce::String tag;
    CDjDeckTelemetry tel {};
    float lastAngle = 0.0f;
    bool scratching = false;
};

class DjDeckComponent : public juce::Component,
                        public juce::FileDragAndDropTarget
{
public:
    DjDeckComponent(ChronomixBridge& bridgeRef, uint32_t deckIdx, juce::Colour deckAccent);
    void refreshFromEngine();
    void updateTelemetry(const CDjDeckTelemetry& t);
    bool loadTrackFile(const juce::File& file);
    std::function<void(const juce::String&)> onStatusOrError;

    void paint(juce::Graphics& g) override;
    void resized() override;
    bool isInterestedInFileDrag(const juce::StringArray& files) override;
    void filesDropped(const juce::StringArray& files, int x, int y) override;

private:
    ChronomixBridge& bridge;
    uint32_t deckIndex = 0;
    juce::Colour accent;
    CDjDeckParams params {};
    CDjDeckTelemetry tel {};
    juce::String trackTitle;
    bool dragHover = false;

    juce::Label titleLabel;
    juce::Label timeLabel;
    juce::Label bpmLabel;
    juce::TextButton loadBtn { "CARGAR PISTA" };
    juce::TextButton ejectBtn { "X" };

    DjWaveformView waveformView;
    DjJogWheel jogWheel;

    juce::TextButton playBtn { "PLAY" };
    juce::TextButton cueBtn { "CUE" };
    juce::TextButton setCueBtn { "SET" };
    juce::TextButton syncBtn { "SYNC" };
    juce::TextButton pitchResetBtn { "0%" };
    juce::Slider pitchSlider;

    std::array<juce::TextButton, 4> hotCueBtns;
    juce::ComboBox loopBeatsBox;
    juce::TextButton loopToggleBtn { "LOOP" };

    std::unique_ptr<juce::FileChooser> fileChooser;
    void pushParams();
};

class DjPanel : public juce::Component
{
public:
    DjPanel(ChronomixBridge& bridgeRef);
    void updateTelemetry();
    bool loadTrackOnDeck(uint32_t deck, const juce::File& file);
    std::function<void(const juce::String&)> onStatusOrError;

    void paint(juce::Graphics& g) override;
    void resized() override;

private:
    ChronomixBridge& bridge;
    DjDeckComponent deckA;
    DjDeckComponent deckB;

    CDjDeckParams paramsA {};
    CDjDeckParams paramsB {};
    CDjMixerParams mixerParams {};
    CDjTelemetry lastTel {};

    struct MixerStripControls
    {
        juce::Slider trimKnob;
        juce::Slider eqHigh;
        juce::Slider eqMid;
        juce::Slider eqLow;
        juce::TextButton killHigh { "K" };
        juce::TextButton killMid { "K" };
        juce::TextButton killLow { "K" };
        juce::Slider filterKnob;
        juce::Slider volFader;
        juce::Label headerLabel;
    };

    MixerStripControls stripA;
    MixerStripControls stripB;

    juce::Slider masterKnob;
    juce::Slider crossfader;
    juce::ComboBox curveBox;
    juce::TextButton autoFadeABtn { "<< MIX A" };
    juce::TextButton autoFadeBBtn { "MIX B >>" };

    juce::Rectangle<float> meterARect;
    juce::Rectangle<float> meterMasterRect;
    juce::Rectangle<float> meterBRect;

    void setupStrip(MixerStripControls& strip, uint32_t deck, const juce::String& name, juce::Colour col);
    void pushDeckStripParams(uint32_t deck);
    void pushMixerParams();
};
