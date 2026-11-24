#include "MainComponent.h"

MainComponent::MainComponent(ChronomixBridge& bridgeRef)
    : bridge(bridgeRef),
      mixerPanel(bridgeRef),
      midiPanel(bridgeRef),
      djPanel(bridgeRef),
      libraryPanel(bridgeRef)
{
    mixerPanel.onNotify = [this](const juce::String& msg)
    {
        if (msg.containsIgnoreCase("error") || msg.containsIgnoreCase("wineg++"))
            activeError = msg;
        else
        {
            activeError.clear();
            statusLabel.setText(msg, juce::dontSendNotification);
        }
    };
    mixerPanel.onRefreshLibrary = [this] { libraryPanel.refreshFiles(); };

    djPanel.onStatusOrError = [this](const juce::String& msg)
    {
        if (msg.containsIgnoreCase("error"))
            activeError = msg;
        else
        {
            activeError.clear();
            statusLabel.setText(msg, juce::dontSendNotification);
        }
    };

    libraryPanel.onAssignToMixer = [this](const juce::File& file)
    {
        mixerPanel.assignMediaFileToNewOrSelectedChannel(file);
        setSection(AppSection::Mixer);
    };

    libraryPanel.onAssignToDjDeck = [this](uint32_t deck, const juce::File& file)
    {
        djPanel.loadTrackOnDeck(deck, file);
        setSection(AppSection::Dj);
    };

    addAndMakeVisible(mixerPanel);
    addChildComponent(midiPanel);
    addChildComponent(djPanel);
    addChildComponent(libraryPanel);

    const char* navLabels[4] = { "MIXER", "MIDI", "DJ", "BIBLIOTECA" };
    const juce::Colour navAccents[4] = {
        StudioColors::cyan,
        StudioColors::orange,
        StudioColors::purple,
        StudioColors::green
    };
    for (size_t i = 0; i < 4; ++i)
    {
        navBtns[i].setButtonText(navLabels[i]);
        navBtns[i].setColour(juce::TextButton::buttonOnColourId, navAccents[i]);
        navBtns[i].setColour(juce::TextButton::textColourOnId, juce::Colours::black);
        navBtns[i].onClick = [this, i] { setSection(static_cast<AppSection>(i)); };
        addAndMakeVisible(navBtns[i]);
    }

    statusLabel.setFont(juce::FontOptions(11.5f));
    statusLabel.setColour(juce::Label::textColourId, StudioColors::textSecond);
    statusLabel.setText(bridge.getStatusText(), juce::dontSendNotification);
    addAndMakeVisible(statusLabel);

    statsLabel.setFont(juce::FontOptions(11.0f));
    statsLabel.setColour(juce::Label::textColourId, StudioColors::textMuted);
    statsLabel.setJustificationType(juce::Justification::centredRight);
    addAndMakeVisible(statsLabel);

    dismissErrBtn.onClick = [this]
    {
        activeError.clear();
        dismissErrBtn.setVisible(false);
        statusLabel.setColour(juce::Label::textColourId, StudioColors::textSecond);
        statusLabel.setText(bridge.getStatusText(), juce::dontSendNotification);
    };
    addChildComponent(dismissErrBtn);

    setSection(AppSection::Mixer);
    startTimerHz(30);
}

MainComponent::~MainComponent()
{
    stopTimer();
}

void MainComponent::setSection(AppSection section)
{
    activeSection = section;
    mixerPanel.setVisible(section == AppSection::Mixer);
    midiPanel.setVisible(section == AppSection::Midi);
    djPanel.setVisible(section == AppSection::Dj);
    libraryPanel.setVisible(section == AppSection::Library);

    for (size_t i = 0; i < 4; ++i)
        navBtns[i].setToggleState(static_cast<int>(section) == static_cast<int>(i), juce::dontSendNotification);

    if (section == AppSection::Mixer)
        mixerPanel.refreshState();
    else if (section == AppSection::Midi)
        midiPanel.refreshFromEngine();
    else if (section == AppSection::Library)
        libraryPanel.refreshFiles();

    repaint();
}

void MainComponent::timerCallback()
{
    bridge.tick();

    auto err = bridge.takeLastError();
    if (err.isNotEmpty())
        activeError = err;

    chronomix_get_mixer_telemetry(bridge.raw(), &mixerTel);
    if (activeSection == AppSection::Mixer)
        mixerPanel.updateTelemetry(mixerTel);
    else if (activeSection == AppSection::Dj)
        djPanel.updateTelemetry();
    else if (activeSection == AppSection::Midi)
        midiPanel.updatePlayhead();

    if (activeError.isNotEmpty())
    {
        statusLabel.setColour(juce::Label::textColourId, StudioColors::red);
        statusLabel.setText("ERROR: " + activeError, juce::dontSendNotification);
        dismissErrBtn.setVisible(true);
    }
    else
    {
        dismissErrBtn.setVisible(false);
        statusLabel.setColour(juce::Label::textColourId, StudioColors::textSecond);
        auto st = bridge.getStatusText();
        if (st.isNotEmpty())
            statusLabel.setText(st, juce::dontSendNotification);
    }

    juce::String statTxt = (mixerTel.is_audio_running != 0 ? "LIVE " : "IDLE ") +
                           juce::String(mixerTel.sample_rate) + " Hz | " +
                           juce::String(mixerTel.block_size) + " smp | XRUN: " +
                           juce::String(static_cast<int64_t>(mixerTel.underruns + mixerTel.overruns));
    statsLabel.setText(statTxt, juce::dontSendNotification);
}

void MainComponent::paint(juce::Graphics& g)
{
    g.fillAll(StudioColors::bgDeep);
    auto b = getLocalBounds();
    auto statusBar = b.removeFromBottom(24);
    auto navBar = b.removeFromBottom(40);

    g.setColour(StudioColors::bgBezel);
    g.fillRect(navBar);
    g.setColour(StudioColors::borderSubtle);
    g.drawHorizontalLine(navBar.getY(), 0.0f, static_cast<float>(getWidth()));

    g.setColour(StudioColors::bgChassis);
    g.fillRect(statusBar);
    g.setColour(StudioColors::borderSubtle);
    g.drawHorizontalLine(statusBar.getY(), 0.0f, static_cast<float>(getWidth()));

    auto dotCol = activeError.isNotEmpty() ? StudioColors::red
                : (mixerTel.is_audio_running != 0 ? StudioColors::green : StudioColors::cyan);
    g.setColour(dotCol);
    g.fillEllipse(8.0f, static_cast<float>(statusBar.getCentreY()) - 4.0f, 8.0f, 8.0f);
}

void MainComponent::resized()
{
    auto area = getLocalBounds();
    auto statusBar = area.removeFromBottom(24);
    auto navBar = area.removeFromBottom(40);

    mixerPanel.setBounds(area);
    midiPanel.setBounds(area);
    djPanel.setBounds(area);
    libraryPanel.setBounds(area);

    int navBtnW = juce::jlimit(88, 150, (navBar.getWidth() - 40) / 4);
    int totalNavW = navBtnW * 4 + 18;
    int startX = juce::jmax(8, (navBar.getWidth() - totalNavW) / 2);
    for (int i = 0; i < 4; ++i)
        navBtns[static_cast<size_t>(i)].setBounds(startX + i * (navBtnW + 6), navBar.getY() + 5, navBtnW, 30);

    statusBar.removeFromLeft(22);
    statsLabel.setBounds(statusBar.removeFromRight(230).reduced(4, 2));
    dismissErrBtn.setBounds(statusBar.removeFromRight(24).reduced(2));
    statusLabel.setBounds(statusBar.reduced(4, 2));
}
