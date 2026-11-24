#pragma once

#include <juce_gui_basics/juce_gui_basics.h>
#include "FfiBridge.h"
#include "ChronomixLookAndFeel.h"
#include "MixerPanel.h"
#include "MidiPanel.h"
#include "DjPanel.h"
#include "LibraryPanel.h"
#include <array>

enum class AppSection
{
    Mixer = 0,
    Midi,
    Dj,
    Library
};

class MainComponent : public juce::Component,
                      private juce::Timer
{
public:
    MainComponent(ChronomixBridge& bridgeRef);
    ~MainComponent() override;

    void paint(juce::Graphics& g) override;
    void resized() override;

private:
    ChronomixBridge& bridge;
    AppSection activeSection = AppSection::Mixer;

    MixerPanel mixerPanel;
    MidiPanel midiPanel;
    DjPanel djPanel;
    LibraryPanel libraryPanel;

    std::array<juce::TextButton, 4> navBtns;
    juce::Label statusLabel;
    juce::Label statsLabel;
    juce::TextButton dismissErrBtn { "X" };

    juce::String activeError;
    CMixerTelemetry mixerTel {};

    void setSection(AppSection section);
    void timerCallback() override;
};
