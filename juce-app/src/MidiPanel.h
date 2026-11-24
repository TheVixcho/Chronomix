#pragma once

#include <juce_gui_basics/juce_gui_basics.h>
#include "FfiBridge.h"
#include "ChronomixLookAndFeel.h"
#include <functional>
#include <vector>

struct UiMidiNote
{
    uint8_t midi = 60;
    int startStep = 0;
    int lengthSteps = 1;
};

struct UiDrumTrack
{
    juce::String name;
    bool steps[16] {};
};

class PianoRollGridView : public juce::Component
{
public:
    void setState(const std::vector<UiMidiNote>& n, float playhead, bool playing);
    std::function<void(uint8_t, int)> onToggleCell;

    void paint(juce::Graphics& g) override;
    void mouseDown(const juce::MouseEvent& e) override;

private:
    std::vector<UiMidiNote> notes;
    float playheadSteps = 0.0f;
    bool isPlaying = false;
    static constexpr int kMinMidi = 48;
    static constexpr int kMaxMidi = 71;
    static constexpr int kKeyWidth = 42;
};

class DrumSequencerGridView : public juce::Component
{
public:
    void setState(const std::vector<UiDrumTrack>& tr, int currentStep, bool playing);
    std::function<void(int, int)> onToggleStep;

    void paint(juce::Graphics& g) override;
    void mouseDown(const juce::MouseEvent& e) override;

private:
    std::vector<UiDrumTrack> tracks;
    int activeStep = 0;
    bool isPlaying = false;
    static constexpr int kNameWidth = 86;
};

class VirtualKeyboardView : public juce::Component
{
public:
    VirtualKeyboardView();
    std::function<void(uint8_t)> onNotePress;

    void paint(juce::Graphics& g) override;
    void mouseDown(const juce::MouseEvent& e) override;
    void mouseUp(const juce::MouseEvent& e) override;
    bool keyPressed(const juce::KeyPress& key) override;

private:
    int pressedMidi = -1;
    static constexpr int kMinMidi = 48;
    static constexpr int kMaxMidi = 71;
    int hitTestKey(juce::Point<int> pos) const;
};

class MidiPanel : public juce::Component
{
public:
    MidiPanel(ChronomixBridge& bridgeRef);
    void refreshFromEngine();
    void updatePlayhead();

    void paint(juce::Graphics& g) override;
    void resized() override;

private:
    ChronomixBridge& bridge;
    juce::ComboBox sessionBox;
    juce::TextButton addSessionBtn { "+ Sesión" };
    juce::TextButton delSessionBtn { "-" };

    juce::TextButton playBtn { "PLAY" };
    juce::TextButton stopBtn { "STOP" };
    juce::Slider bpmSlider;
    juce::Slider swingSlider;
    juce::ToggleButton drumsToggle { "Drums" };
    juce::TextButton demoBtn { "Demo" };
    juce::TextButton clearBtn { "Limpiar" };

    juce::ComboBox presetBox;
    juce::Slider cutoffSlider;
    juce::Slider driveSlider;
    juce::Slider releaseSlider;
    juce::Slider delaySlider;

    DrumSequencerGridView drumGrid;
    PianoRollGridView pianoRoll;
    VirtualKeyboardView keyboard;

    bool playingState = false;
    void pushTransport(bool play);
    void pushSynth();
};
