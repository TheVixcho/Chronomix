#include "MidiPanel.h"

static bool isBlackMidi(int midi)
{
    int m = midi % 12;
    return m == 1 || m == 3 || m == 6 || m == 8 || m == 10;
}

static juce::String midiLabel(int midi)
{
    static const char* names[12] = { "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B" };
    return juce::String(names[midi % 12]) + juce::String((midi / 12) - 1);
}

void PianoRollGridView::setState(const std::vector<UiMidiNote>& n, float playhead, bool playing)
{
    notes = n;
    playheadSteps = playhead;
    isPlaying = playing;
    repaint();
}

void PianoRollGridView::mouseDown(const juce::MouseEvent& e)
{
    if (e.x < kKeyWidth || !onToggleCell)
        return;
    const int numRows = kMaxMidi - kMinMidi + 1;
    const float rowH = static_cast<float>(getHeight()) / static_cast<float>(numRows);
    const float colW = static_cast<float>(getWidth() - kKeyWidth) / 16.0f;
    int row = juce::jlimit(0, numRows - 1, static_cast<int>(static_cast<float>(e.y) / rowH));
    int step = juce::jlimit(0, 15, static_cast<int>(static_cast<float>(e.x - kKeyWidth) / colW));
    uint8_t midi = static_cast<uint8_t>(kMaxMidi - row);
    onToggleCell(midi, step);
}

void PianoRollGridView::paint(juce::Graphics& g)
{
    auto b = getLocalBounds().toFloat();
    g.setColour(juce::Colour(0xff0d0f14));
    g.fillRect(b);

    const int numRows = kMaxMidi - kMinMidi + 1;
    const float rowH = b.getHeight() / static_cast<float>(numRows);
    const float colW = (b.getWidth() - static_cast<float>(kKeyWidth)) / 16.0f;

    for (int r = 0; r < numRows; ++r)
    {
        int midi = kMaxMidi - r;
        float y = static_cast<float>(r) * rowH;
        bool blk = isBlackMidi(midi);

        g.setColour(blk ? juce::Colour(0xff14161e) : juce::Colour(0xff191c25));
        g.fillRect(static_cast<float>(kKeyWidth), y, b.getWidth() - static_cast<float>(kKeyWidth), rowH);

        g.setColour(blk ? juce::Colour(0xff181a22) : juce::Colour(0xffcfd4e0));
        g.fillRect(0.0f, y, static_cast<float>(kKeyWidth), rowH - 0.5f);
        g.setColour(blk ? StudioColors::textMuted : juce::Colours::black);
        g.setFont(juce::FontOptions(8.5f));
        g.drawText(midiLabel(midi), 2, static_cast<int>(y), kKeyWidth - 4, static_cast<int>(rowH), juce::Justification::centredRight);

        g.setColour(juce::Colour(0xff222634));
        g.drawHorizontalLine(static_cast<int>(y), 0.0f, b.getWidth());
    }

    for (int s = 0; s < 16; ++s)
    {
        float x = static_cast<float>(kKeyWidth) + static_cast<float>(s) * colW;
        g.setColour((s % 4 == 0) ? StudioColors::borderBright : juce::Colour(0xff232734));
        g.drawVerticalLine(static_cast<int>(x), 0.0f, b.getHeight());
    }

    for (const auto& n : notes)
    {
        if (n.midi < kMinMidi || n.midi > kMaxMidi)
            continue;
        int r = kMaxMidi - static_cast<int>(n.midi);
        float x = static_cast<float>(kKeyWidth) + static_cast<float>(n.startStep) * colW + 1.0f;
        float y = static_cast<float>(r) * rowH + 1.0f;
        float w = static_cast<float>(juce::jmax(1, n.lengthSteps)) * colW - 2.0f;
        g.setColour(StudioColors::green);
        g.fillRect(x, y, w, juce::jmax(2.0f, rowH - 2.0f));
        g.setColour(juce::Colours::black.withAlpha(0.5f));
        g.drawRect(x, y, w, juce::jmax(2.0f, rowH - 2.0f), 1.0f);
    }

    if (isPlaying)
    {
        float px = static_cast<float>(kKeyWidth) + (playheadSteps / 16.0f) * (b.getWidth() - static_cast<float>(kKeyWidth));
        g.setColour(StudioColors::orange);
        g.fillRect(px - 1.0f, 0.0f, 2.5f, b.getHeight());
    }

    g.setColour(StudioColors::borderSubtle);
    g.drawRect(b, 1.0f);
}

void DrumSequencerGridView::setState(const std::vector<UiDrumTrack>& tr, int currentStep, bool playing)
{
    tracks = tr;
    activeStep = currentStep;
    isPlaying = playing;
    repaint();
}

void DrumSequencerGridView::mouseDown(const juce::MouseEvent& e)
{
    if (e.x < kNameWidth || tracks.empty() || !onToggleStep)
        return;
    float rowH = static_cast<float>(getHeight()) / static_cast<float>(tracks.size());
    float colW = static_cast<float>(getWidth() - kNameWidth) / 16.0f;
    int r = juce::jlimit(0, static_cast<int>(tracks.size()) - 1, static_cast<int>(static_cast<float>(e.y) / rowH));
    int s = juce::jlimit(0, 15, static_cast<int>(static_cast<float>(e.x - kNameWidth) / colW));
    onToggleStep(r, s);
}

void DrumSequencerGridView::paint(juce::Graphics& g)
{
    auto b = getLocalBounds().toFloat();
    g.setColour(StudioColors::bgBezel);
    g.fillRect(b);
    if (tracks.empty())
        return;

    float rowH = b.getHeight() / static_cast<float>(tracks.size());
    float colW = (b.getWidth() - static_cast<float>(kNameWidth)) / 16.0f;

    for (int r = 0; r < static_cast<int>(tracks.size()); ++r)
    {
        float y = static_cast<float>(r) * rowH;
        g.setColour(StudioColors::bgCard);
        g.fillRect(2.0f, y + 1.0f, static_cast<float>(kNameWidth - 4), rowH - 2.0f);
        g.setColour(StudioColors::textPrimary);
        g.setFont(juce::FontOptions(10.5f, juce::Font::bold));
        g.drawFittedText(tracks[static_cast<size_t>(r)].name, 6, static_cast<int>(y),
                         kNameWidth - 10, static_cast<int>(rowH), juce::Justification::centredLeft, 1);

        for (int s = 0; s < 16; ++s)
        {
            float x = static_cast<float>(kNameWidth) + static_cast<float>(s) * colW;
            auto cell = juce::Rectangle<float>(x + 2.0f, y + 2.0f, colW - 4.0f, rowH - 4.0f);
            bool on = tracks[static_cast<size_t>(r)].steps[s];
            bool groupAlt = ((s / 4) % 2) == 1;
            auto col = on ? ((s % 4 == 0) ? StudioColors::orange : StudioColors::cyan)
                          : (groupAlt ? juce::Colour(0xff232531) : juce::Colour(0xff1a1c25));
            g.setColour(col);
            g.fillRect(cell);
            if (isPlaying && s == activeStep)
            {
                g.setColour(juce::Colours::white);
                g.drawRect(cell, 1.5f);
            }
            else
            {
                g.setColour(StudioColors::borderSubtle);
                g.drawRect(cell, 1.0f);
            }
        }
    }
    g.setColour(StudioColors::borderSubtle);
    g.drawRect(b, 1.0f);
}

VirtualKeyboardView::VirtualKeyboardView()
{
    setWantsKeyboardFocus(true);
}

int VirtualKeyboardView::hitTestKey(juce::Point<int> pos) const
{
    std::vector<int> whites;
    for (int m = kMinMidi; m <= kMaxMidi; ++m)
        if (!isBlackMidi(m))
            whites.push_back(m);
    if (whites.empty())
        return -1;

    float wkW = static_cast<float>(getWidth()) / static_cast<float>(whites.size());
    float bkH = static_cast<float>(getHeight()) * 0.62f;

    if (static_cast<float>(pos.y) <= bkH)
    {
        int wIdx = 0;
        for (int m = kMinMidi; m <= kMaxMidi; ++m)
        {
            if (isBlackMidi(m))
            {
                float bx = static_cast<float>(wIdx) * wkW - wkW * 0.32f;
                if (static_cast<float>(pos.x) >= bx && static_cast<float>(pos.x) <= bx + wkW * 0.64f)
                    return m;
            }
            else
            {
                ++wIdx;
            }
        }
    }
    int idx = juce::jlimit(0, static_cast<int>(whites.size()) - 1, static_cast<int>(static_cast<float>(pos.x) / wkW));
    return whites[static_cast<size_t>(idx)];
}

void VirtualKeyboardView::mouseDown(const juce::MouseEvent& e)
{
    grabKeyboardFocus();
    pressedMidi = hitTestKey(e.getPosition());
    if (pressedMidi >= 0 && onNotePress)
        onNotePress(static_cast<uint8_t>(pressedMidi));
    repaint();
}

void VirtualKeyboardView::mouseUp(const juce::MouseEvent&)
{
    pressedMidi = -1;
    repaint();
}

bool VirtualKeyboardView::keyPressed(const juce::KeyPress& key)
{
    static const struct { char ch; uint8_t midi; } kMap[] = {
        { 'a', 60 }, { 'w', 61 }, { 's', 62 }, { 'e', 63 }, { 'd', 64 }, { 'f', 65 },
        { 't', 66 }, { 'g', 67 }, { 'y', 68 }, { 'h', 69 }, { 'u', 70 }, { 'j', 71 },
        { 'k', 72 }, { 'z', 48 }, { 'x', 50 }, { 'c', 52 }, { 'v', 53 }, { 'b', 55 }, { 'n', 57 }, { 'm', 59 }
    };
    auto ch = static_cast<char>(juce::CharacterFunctions::toLowerCase(key.getTextCharacter()));
    for (const auto& entry : kMap)
    {
        if (entry.ch == ch)
        {
            pressedMidi = entry.midi;
            if (onNotePress)
                onNotePress(entry.midi);
            repaint();
            return true;
        }
    }
    return false;
}

void VirtualKeyboardView::paint(juce::Graphics& g)
{
    auto b = getLocalBounds().toFloat();
    std::vector<int> whites;
    for (int m = kMinMidi; m <= kMaxMidi; ++m)
        if (!isBlackMidi(m))
            whites.push_back(m);
    if (whites.empty())
        return;

    float wkW = b.getWidth() / static_cast<float>(whites.size());
    for (size_t i = 0; i < whites.size(); ++i)
    {
        int m = whites[i];
        float x = static_cast<float>(i) * wkW;
        g.setColour(m == pressedMidi ? StudioColors::orange : juce::Colour(0xffe6e8ef));
        g.fillRect(x, 0.0f, wkW - 1.0f, b.getHeight());
        g.setColour(juce::Colour(0xff252834));
        g.drawRect(x, 0.0f, wkW, b.getHeight(), 1.0f);
        g.setColour(juce::Colours::black);
        g.setFont(juce::FontOptions(9.0f));
        g.drawText(midiLabel(m), static_cast<int>(x), static_cast<int>(b.getHeight() - 16),
                   static_cast<int>(wkW), 14, juce::Justification::centred);
    }

    int wIdx = 0;
    float bkH = b.getHeight() * 0.62f;
    for (int m = kMinMidi; m <= kMaxMidi; ++m)
    {
        if (isBlackMidi(m))
        {
            float bx = static_cast<float>(wIdx) * wkW - wkW * 0.32f;
            g.setColour(m == pressedMidi ? StudioColors::orange : juce::Colour(0xff14161d));
            g.fillRect(bx, 0.0f, wkW * 0.64f, bkH);
            g.setColour(StudioColors::borderBright);
            g.drawRect(bx, 0.0f, wkW * 0.64f, bkH, 1.0f);
        }
        else
        {
            ++wIdx;
        }
    }
}

MidiPanel::MidiPanel(ChronomixBridge& bridgeRef)
    : bridge(bridgeRef)
{
    sessionBox.onChange = [this]
    {
        int idx = sessionBox.getSelectedItemIndex();
        if (idx >= 0)
        {
            chronomix_midi_select_session(bridge.raw(), static_cast<uint32_t>(idx));
            refreshFromEngine();
        }
    };
    addAndMakeVisible(sessionBox);

    addSessionBtn.onClick = [this]
    {
        chronomix_midi_add_session(bridge.raw());
        refreshFromEngine();
    };
    addAndMakeVisible(addSessionBtn);

    delSessionBtn.onClick = [this]
    {
        chronomix_midi_remove_active_session(bridge.raw());
        refreshFromEngine();
    };
    addAndMakeVisible(delSessionBtn);

    playBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::green);
    playBtn.onClick = [this] { pushTransport(true); };
    addAndMakeVisible(playBtn);

    stopBtn.onClick = [this] { pushTransport(false); };
    addAndMakeVisible(stopBtn);

    auto initSlider = [&](juce::Slider& s, double minV, double maxV, double defV, const juce::String& suffix)
    {
        s.setSliderStyle(juce::Slider::LinearHorizontal);
        s.setRange(minV, maxV, 0.01);
        s.setValue(defV, juce::dontSendNotification);
        s.setTextValueSuffix(suffix);
        s.setTextBoxStyle(juce::Slider::TextBoxRight, false, 52, 20);
        addAndMakeVisible(s);
    };

    initSlider(bpmSlider, 40.0, 240.0, 124.0, " BPM");
    bpmSlider.setRange(40.0, 240.0, 1.0);
    bpmSlider.onValueChange = [this] { pushTransport(playingState); };

    initSlider(swingSlider, 0.0, 0.6, 0.0, " Sw");
    swingSlider.onValueChange = [this] { pushTransport(playingState); };

    drumsToggle.setToggleState(true, juce::dontSendNotification);
    drumsToggle.onClick = [this] { pushTransport(playingState); };
    addAndMakeVisible(drumsToggle);

    demoBtn.onClick = [this]
    {
        chronomix_midi_load_demo_melody(bridge.raw());
        refreshFromEngine();
    };
    addAndMakeVisible(demoBtn);

    clearBtn.onClick = [this]
    {
        chronomix_midi_clear_notes(bridge.raw());
        refreshFromEngine();
    };
    addAndMakeVisible(clearBtn);

    presetBox.addItem("Studio Keys / Analog Lead", 1);
    presetBox.addItem("Electric Piano FM", 2);
    presetBox.addItem("Warm Saw Pad", 3);
    presetBox.addItem("Pluck Arp", 4);
    presetBox.addItem("808 Sub Bass", 5);
    presetBox.setSelectedId(1, juce::dontSendNotification);
    presetBox.onChange = [this] { pushSynth(); };
    addAndMakeVisible(presetBox);

    initSlider(cutoffSlider, 300.0, 14000.0, 5200.0, " Hz");
    cutoffSlider.onValueChange = [this] { pushSynth(); };
    initSlider(driveSlider, 0.0, 1.0, 0.2, " Drv");
    driveSlider.onValueChange = [this] { pushSynth(); };
    initSlider(releaseSlider, 0.04, 1.2, 0.32, " s");
    releaseSlider.onValueChange = [this] { pushSynth(); };
    initSlider(delaySlider, 0.0, 0.8, 0.18, " Dly");
    delaySlider.onValueChange = [this] { pushSynth(); };

    drumGrid.onToggleStep = [this](int tr, int step)
    {
        chronomix_midi_toggle_step(bridge.raw(), static_cast<uint32_t>(tr), static_cast<uint32_t>(step));
        refreshFromEngine();
    };
    addAndMakeVisible(drumGrid);

    pianoRoll.onToggleCell = [this](uint8_t midi, int step)
    {
        chronomix_midi_toggle_note(bridge.raw(), midi, static_cast<uint32_t>(step), 2);
        refreshFromEngine();
    };
    addAndMakeVisible(pianoRoll);

    keyboard.onNotePress = [this](uint8_t midi)
    {
        chronomix_midi_trigger_key(bridge.raw(), midi);
    };
    addAndMakeVisible(keyboard);

    refreshFromEngine();
}

void MidiPanel::pushTransport(bool play)
{
    playingState = play;
    chronomix_midi_set_transport(
        bridge.raw(),
        play ? 1 : 0,
        static_cast<float>(bpmSlider.getValue()),
        static_cast<float>(swingSlider.getValue()),
        drumsToggle.getToggleState() ? 1 : 0);
    playBtn.setToggleState(playingState, juce::dontSendNotification);
}

void MidiPanel::pushSynth()
{
    uint8_t preset = static_cast<uint8_t>(juce::jlimit(0, 4, presetBox.getSelectedId() - 1));
    chronomix_midi_set_synth(
        bridge.raw(),
        preset,
        0.008f,
        static_cast<float>(releaseSlider.getValue()),
        static_cast<float>(cutoffSlider.getValue()),
        0.3f,
        static_cast<float>(driveSlider.getValue()),
        static_cast<float>(delaySlider.getValue()),
        240.0f);
}

void MidiPanel::refreshFromEngine()
{
    auto st = ChronomixBridge::takeJson(chronomix_midi_get_state_json(bridge.raw()));
    auto* sessions = st["sessions"].getArray();
    if (sessions == nullptr || sessions->isEmpty())
        return;

    int activeIdx = static_cast<int>(st["active_session_idx"]);
    sessionBox.clear(juce::dontSendNotification);
    for (int i = 0; i < sessions->size(); ++i)
        sessionBox.addItem((*sessions)[i]["name"].toString(), i + 1);
    sessionBox.setSelectedId(juce::jlimit(1, sessions->size(), activeIdx + 1), juce::dontSendNotification);

    const auto& s = (*sessions)[juce::jlimit(0, sessions->size() - 1, activeIdx)];
    playingState = static_cast<bool>(s["is_playing"]);
    playBtn.setToggleState(playingState, juce::dontSendNotification);
    bpmSlider.setValue(static_cast<double>(s["bpm"]), juce::dontSendNotification);
    swingSlider.setValue(static_cast<double>(s["swing"]), juce::dontSendNotification);
    drumsToggle.setToggleState(static_cast<bool>(s["drums_enabled"]), juce::dontSendNotification);

    auto synth = s["synth"];
    presetBox.setSelectedId(static_cast<int>(synth["preset"]) + 1, juce::dontSendNotification);
    cutoffSlider.setValue(static_cast<double>(synth["cutoff"]), juce::dontSendNotification);
    driveSlider.setValue(static_cast<double>(synth["drive"]), juce::dontSendNotification);
    releaseSlider.setValue(static_cast<double>(synth["release"]), juce::dontSendNotification);
    delaySlider.setValue(static_cast<double>(synth["delay_send"]), juce::dontSendNotification);

    std::vector<UiDrumTrack> tracks;
    if (auto* arr = s["tracks"].getArray())
    {
        for (const auto& t : *arr)
        {
            UiDrumTrack dt;
            dt.name = t["name"].toString();
            if (auto* steps = t["steps"].getArray())
                for (int k = 0; k < juce::jmin(16, steps->size()); ++k)
                    dt.steps[k] = static_cast<bool>((*steps)[k]);
            tracks.push_back(std::move(dt));
        }
    }
    drumGrid.setState(tracks, static_cast<int>(s["current_step"]), playingState);

    std::vector<UiMidiNote> notes;
    if (auto* arr = s["notes"].getArray())
    {
        for (const auto& n : *arr)
        {
            UiMidiNote un;
            un.midi = static_cast<uint8_t>(static_cast<int>(n["midi"]));
            un.startStep = static_cast<int>(n["start_step"]);
            un.lengthSteps = static_cast<int>(n["length_steps"]);
            notes.push_back(un);
        }
    }
    pianoRoll.setState(notes, static_cast<float>(static_cast<double>(s["playhead_pos_steps"])), playingState);
}

void MidiPanel::updatePlayhead()
{
    if (playingState)
        refreshFromEngine();
}

void MidiPanel::paint(juce::Graphics& g)
{
    g.fillAll(StudioColors::bgDeep);
}

void MidiPanel::resized()
{
    auto area = getLocalBounds().reduced(8);

    auto topBar = area.removeFromTop(28);
    sessionBox.setBounds(topBar.removeFromLeft(120));
    topBar.removeFromLeft(4);
    addSessionBtn.setBounds(topBar.removeFromLeft(78));
    topBar.removeFromLeft(2);
    delSessionBtn.setBounds(topBar.removeFromLeft(26));
    topBar.removeFromLeft(10);
    playBtn.setBounds(topBar.removeFromLeft(62));
    topBar.removeFromLeft(4);
    stopBtn.setBounds(topBar.removeFromLeft(62));
    topBar.removeFromLeft(8);
    bpmSlider.setBounds(topBar.removeFromLeft(140));
    topBar.removeFromLeft(6);
    swingSlider.setBounds(topBar.removeFromLeft(110));
    topBar.removeFromLeft(6);
    drumsToggle.setBounds(topBar.removeFromLeft(70));
    clearBtn.setBounds(topBar.removeFromRight(68));
    topBar.removeFromRight(4);
    demoBtn.setBounds(topBar.removeFromRight(62));

    area.removeFromTop(6);
    auto synthBar = area.removeFromTop(26);
    presetBox.setBounds(synthBar.removeFromLeft(185));
    synthBar.removeFromLeft(6);
    int slW = juce::jmax(80, synthBar.getWidth() / 4 - 4);
    cutoffSlider.setBounds(synthBar.removeFromLeft(slW));
    synthBar.removeFromLeft(4);
    driveSlider.setBounds(synthBar.removeFromLeft(slW));
    synthBar.removeFromLeft(4);
    releaseSlider.setBounds(synthBar.removeFromLeft(slW));
    synthBar.removeFromLeft(4);
    delaySlider.setBounds(synthBar);

    area.removeFromTop(6);
    int kbH = juce::jlimit(64, 96, area.getHeight() / 5);
    keyboard.setBounds(area.removeFromBottom(kbH));
    area.removeFromBottom(6);

    int drumH = juce::jlimit(96, 138, area.getHeight() / 3);
    drumGrid.setBounds(area.removeFromTop(drumH));
    area.removeFromTop(6);
    pianoRoll.setBounds(area);
}
