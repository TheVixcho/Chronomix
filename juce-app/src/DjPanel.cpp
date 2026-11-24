#include "DjPanel.h"
#include <cmath>

static juce::String formatTimeMmSs(double secs)
{
    if (secs < 0.0)
        secs = 0.0;
    int totalTenths = static_cast<int>(secs * 10.0);
    int mins = totalTenths / 600;
    int remSecs = (totalTenths / 10) % 60;
    int tenths = totalTenths % 10;
    return juce::String::formatted("%02d:%02d.%d", mins, remSecs, tenths);
}

DjWaveformView::DjWaveformView(juce::Colour deckAccent)
    : accent(deckAccent)
{
}

void DjWaveformView::setWaveform(const std::vector<CWaveformBin>& bins)
{
    waveform = bins;
    repaint();
}

void DjWaveformView::setTelemetry(const CDjDeckTelemetry& t)
{
    tel = t;
    repaint();
}

void DjWaveformView::seekAtX(int x)
{
    if (getWidth() <= 2 || !onSeekNorm)
        return;
    double norm = juce::jlimit(0.0, 1.0, static_cast<double>(x) / static_cast<double>(getWidth()));
    onSeekNorm(norm);
}

void DjWaveformView::mouseDown(const juce::MouseEvent& e) { seekAtX(e.x); }
void DjWaveformView::mouseDrag(const juce::MouseEvent& e) { seekAtX(e.x); }

void DjWaveformView::paint(juce::Graphics& g)
{
    auto bounds = getLocalBounds().toFloat();
    g.setColour(juce::Colour(0xff0a0c10));
    g.fillRect(bounds);
    g.setColour(StudioColors::borderSubtle);
    g.drawRect(bounds, 1.0f);

    if (waveform.empty() || tel.has_track == 0 || tel.total_frames == 0)
    {
        g.setColour(StudioColors::textMuted);
        g.setFont(juce::FontOptions(11.5f));
        g.drawFittedText("Arrastra un archivo WAV / MP3 / FLAC / OGG o presiona CARGAR PISTA",
                         getLocalBounds().reduced(6), juce::Justification::centred, 2);
        return;
    }

    auto inner = bounds.reduced(2.0f);
    auto overviewRect = inner.removeFromTop(inner.getHeight() * 0.42f);
    inner.removeFromTop(2.0f);
    auto zoomRect = inner;

    g.setColour(juce::Colour(0xff0f1218));
    g.fillRect(overviewRect);
    g.fillRect(zoomRect);

    const int nBins = static_cast<int>(waveform.size());
    const float binW = overviewRect.getWidth() / static_cast<float>(nBins);
    const float ovMid = overviewRect.getCentreY();
    const float ovHalf = overviewRect.getHeight() * 0.46f;

    for (int i = 0; i < nBins; ++i)
    {
        const auto& b = waveform[static_cast<size_t>(i)];
        float h = juce::jmax(1.0f, b.peak * ovHalf);
        float sum = b.low + b.mid + b.high + 1e-4f;
        float r = juce::jlimit(0.15f, 1.0f, (b.low * 1.4f + b.mid * 0.4f) / sum);
        float gr = juce::jlimit(0.15f, 1.0f, (b.mid * 1.2f + b.high * 0.6f) / sum);
        float bl = juce::jlimit(0.15f, 1.0f, (b.high * 1.5f + b.mid * 0.3f) / sum);
        g.setColour(juce::Colour::fromFloatRGBA(r, gr, bl, 0.90f));
        float x = overviewRect.getX() + static_cast<float>(i) * binW;
        g.fillRect(x, ovMid - h, juce::jmax(1.0f, binW), h * 2.0f);
    }

    const double posNorm = static_cast<double>(tel.pos_frames) / static_cast<double>(tel.total_frames);

    if (tel.loop_enabled != 0 && tel.loop_end_frame > tel.loop_start_frame)
    {
        float lx0 = overviewRect.getX() + static_cast<float>(tel.loop_start_frame / static_cast<double>(tel.total_frames)) * overviewRect.getWidth();
        float lx1 = overviewRect.getX() + static_cast<float>(tel.loop_end_frame / static_cast<double>(tel.total_frames)) * overviewRect.getWidth();
        g.setColour(StudioColors::green.withAlpha(0.28f));
        g.fillRect(lx0, overviewRect.getY(), juce::jmax(2.0f, lx1 - lx0), overviewRect.getHeight());
    }

    for (int c = 0; c < 4; ++c)
    {
        if (tel.hot_cue_set[c] != 0)
        {
            float cx = overviewRect.getX() + static_cast<float>(tel.hot_cues[c] / static_cast<double>(tel.total_frames)) * overviewRect.getWidth();
            g.setColour(StudioColors::yellow);
            g.drawVerticalLine(static_cast<int>(cx), overviewRect.getY(), overviewRect.getBottom());
        }
    }

    float cueX = overviewRect.getX() + static_cast<float>(tel.cue_frame / static_cast<double>(tel.total_frames)) * overviewRect.getWidth();
    g.setColour(StudioColors::red);
    g.drawVerticalLine(static_cast<int>(cueX), overviewRect.getY(), overviewRect.getBottom());

    float playX = overviewRect.getX() + static_cast<float>(posNorm) * overviewRect.getWidth();
    g.setColour(juce::Colours::white);
    g.fillRect(playX - 1.0f, overviewRect.getY(), 2.0f, overviewRect.getHeight());

    const int zoomCols = juce::jmax(32, static_cast<int>(zoomRect.getWidth() / 2.0f));
    const double windowFrac = 0.08;
    const float zMid = zoomRect.getCentreY();
    const float zHalf = zoomRect.getHeight() * 0.45f;
    const float colW = zoomRect.getWidth() / static_cast<float>(zoomCols);

    for (int c = 0; c < zoomCols; ++c)
    {
        double rel = (static_cast<double>(c) / static_cast<double>(zoomCols) - 0.5) * windowFrac;
        double sampleNorm = posNorm + rel;
        if (sampleNorm < 0.0 || sampleNorm >= 1.0)
            continue;
        int bIdx = juce::jlimit(0, nBins - 1, static_cast<int>(sampleNorm * static_cast<double>(nBins)));
        const auto& b = waveform[static_cast<size_t>(bIdx)];
        float h = juce::jmax(1.0f, b.peak * zHalf);
        g.setColour(accent.withAlpha(0.85f));
        float x = zoomRect.getX() + static_cast<float>(c) * colW;
        g.fillRect(x, zMid - h, juce::jmax(1.2f, colW - 0.3f), h * 2.0f);
    }

    g.setColour(StudioColors::orange);
    g.fillRect(zoomRect.getCentreX() - 1.0f, zoomRect.getY(), 2.0f, zoomRect.getHeight());
}

DjJogWheel::DjJogWheel(juce::Colour deckAccent, juce::String deckTag)
    : accent(deckAccent), tag(std::move(deckTag))
{
}

void DjJogWheel::setTelemetry(const CDjDeckTelemetry& t)
{
    tel = t;
    repaint();
}

void DjJogWheel::mouseDown(const juce::MouseEvent& e)
{
    scratching = true;
    auto c = getLocalBounds().toFloat().getCentre();
    lastAngle = std::atan2(static_cast<float>(e.y) - c.y, static_cast<float>(e.x) - c.x);
    repaint();
}

void DjJogWheel::mouseDrag(const juce::MouseEvent& e)
{
    auto c = getLocalBounds().toFloat().getCentre();
    float a = std::atan2(static_cast<float>(e.y) - c.y, static_cast<float>(e.x) - c.x);
    float delta = a - lastAngle;
    while (delta > juce::MathConstants<float>::pi) delta -= juce::MathConstants<float>::twoPi;
    while (delta < -juce::MathConstants<float>::pi) delta += juce::MathConstants<float>::twoPi;
    lastAngle = a;
    if (onNudgeSecs)
        onNudgeSecs(static_cast<double>(delta) * 0.45);
}

void DjJogWheel::mouseUp(const juce::MouseEvent&)
{
    scratching = false;
    repaint();
}

void DjJogWheel::paint(juce::Graphics& g)
{
    auto bounds = getLocalBounds().toFloat().reduced(4.0f);
    float dia = juce::jmin(bounds.getWidth(), bounds.getHeight());
    auto circle = juce::Rectangle<float>(bounds.getCentreX() - dia * 0.5f,
                                         bounds.getCentreY() - dia * 0.5f,
                                         dia, dia);
    float r = dia * 0.5f;
    auto centre = circle.getCentre();

    g.setColour(juce::Colour(0xff0b0c10));
    g.fillEllipse(circle);

    g.setColour(scratching ? StudioColors::orange : (tel.playing != 0 ? accent : StudioColors::borderBright));
    g.drawEllipse(circle.reduced(1.5f), 3.0f);

    for (float ring = 0.85f; ring >= 0.48f; ring -= 0.10f)
    {
        float rr = r * ring;
        g.setColour(juce::Colour(0xff1e222e));
        g.drawEllipse(centre.x - rr, centre.y - rr, rr * 2.0f, rr * 2.0f, 1.0f);
    }

    double secs = (tel.sample_rate > 0) ? (static_cast<double>(tel.pos_frames) / static_cast<double>(tel.sample_rate)) : 0.0;
    float spinAngle = static_cast<float>(std::fmod(secs * 1.8 * juce::MathConstants<double>::twoPi, juce::MathConstants<double>::twoPi));

    float mx = centre.x + std::cos(spinAngle) * (r * 0.72f);
    float my = centre.y + std::sin(spinAngle) * (r * 0.72f);
    g.setColour(accent);
    g.drawLine(centre.x, centre.y, mx, my, 2.5f);
    g.fillEllipse(mx - 4.0f, my - 4.0f, 8.0f, 8.0f);

    float hubR = r * 0.36f;
    g.setColour(juce::Colour(0xff14171f));
    g.fillEllipse(centre.x - hubR, centre.y - hubR, hubR * 2.0f, hubR * 2.0f);
    g.setColour(accent.withAlpha(0.8f));
    g.drawEllipse(centre.x - hubR, centre.y - hubR, hubR * 2.0f, hubR * 2.0f, 1.5f);

    g.setColour(StudioColors::textPrimary);
    g.setFont(juce::FontOptions(11.0f, juce::Font::bold));
    g.drawFittedText(tag, juce::Rectangle<int>(static_cast<int>(centre.x - hubR), static_cast<int>(centre.y - 12),
                                               static_cast<int>(hubR * 2.0f), 14),
                     juce::Justification::centred, 1);
    g.setColour(accent);
    g.setFont(juce::FontOptions(10.0f));
    g.drawFittedText(juce::String(tel.effective_bpm, 1),
                     juce::Rectangle<int>(static_cast<int>(centre.x - hubR), static_cast<int>(centre.y + 1),
                                          static_cast<int>(hubR * 2.0f), 13),
                     juce::Justification::centred, 1);
}

DjDeckComponent::DjDeckComponent(ChronomixBridge& bridgeRef, uint32_t deckIdx, juce::Colour deckAccent)
    : bridge(bridgeRef),
      deckIndex(deckIdx),
      accent(deckAccent),
      waveformView(deckAccent),
      jogWheel(deckAccent, deckIdx == 0 ? "DECK A" : "DECK B")
{
    chronomix_dj_get_deck_params(bridge.raw(), deckIndex, &params);

    titleLabel.setText(deckIndex == 0 ? "DECK A — Sin pista" : "DECK B — Sin pista", juce::dontSendNotification);
    titleLabel.setFont(juce::FontOptions(13.0f, juce::Font::bold));
    titleLabel.setColour(juce::Label::textColourId, accent);
    addAndMakeVisible(titleLabel);

    timeLabel.setText("00:00.0 / 00:00.0", juce::dontSendNotification);
    timeLabel.setFont(juce::FontOptions(11.5f));
    timeLabel.setColour(juce::Label::textColourId, StudioColors::textPrimary);
    addAndMakeVisible(timeLabel);

    bpmLabel.setText("120.0 BPM", juce::dontSendNotification);
    bpmLabel.setFont(juce::FontOptions(12.0f, juce::Font::bold));
    bpmLabel.setColour(juce::Label::textColourId, StudioColors::yellow);
    bpmLabel.setJustificationType(juce::Justification::centredRight);
    addAndMakeVisible(bpmLabel);

    loadBtn.setColour(juce::TextButton::buttonColourId, accent.withAlpha(0.25f));
    loadBtn.onClick = [this]
    {
        fileChooser = std::make_unique<juce::FileChooser>(
            deckIndex == 0 ? "Cargar pista en Deck A" : "Cargar pista en Deck B",
            juce::File(),
            bridge.getSupportedAudioWildcard());
        fileChooser->launchAsync(juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles,
                                 [this](const juce::FileChooser& fc)
                                 {
                                     auto f = fc.getResult();
                                     if (f.existsAsFile())
                                         loadTrackFile(f);
                                 });
    };
    addAndMakeVisible(loadBtn);

    ejectBtn.onClick = [this]
    {
        chronomix_dj_unload(bridge.raw(), deckIndex);
        refreshFromEngine();
    };
    addAndMakeVisible(ejectBtn);

    waveformView.onSeekNorm = [this](double norm)
    {
        chronomix_dj_seek_norm(bridge.raw(), deckIndex, norm);
    };
    addAndMakeVisible(waveformView);

    jogWheel.onNudgeSecs = [this](double d)
    {
        chronomix_dj_nudge_secs(bridge.raw(), deckIndex, d);
    };
    addAndMakeVisible(jogWheel);

    playBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::green);
    playBtn.onClick = [this]
    {
        bool next = (tel.playing == 0);
        chronomix_dj_set_playing(bridge.raw(), deckIndex, next ? 1 : 0);
    };
    addAndMakeVisible(playBtn);

    cueBtn.setColour(juce::TextButton::buttonColourId, StudioColors::red.withAlpha(0.28f));
    cueBtn.onClick = [this]
    {
        chronomix_dj_jump_cue(bridge.raw(), deckIndex, 1);
    };
    addAndMakeVisible(cueBtn);

    setCueBtn.onClick = [this]
    {
        chronomix_dj_set_cue(bridge.raw(), deckIndex);
    };
    addAndMakeVisible(setCueBtn);

    syncBtn.setColour(juce::TextButton::buttonColourId, StudioColors::cyan.withAlpha(0.22f));
    syncBtn.onClick = [this]
    {
        chronomix_dj_sync_bpm(bridge.raw(), deckIndex);
        chronomix_dj_get_deck_params(bridge.raw(), deckIndex, &params);
        pitchSlider.setValue(params.tempo_percent, juce::dontSendNotification);
    };
    addAndMakeVisible(syncBtn);

    pitchResetBtn.onClick = [this]
    {
        params.tempo_percent = 0.0f;
        pitchSlider.setValue(0.0, juce::dontSendNotification);
        pushParams();
    };
    addAndMakeVisible(pitchResetBtn);

    pitchSlider.setSliderStyle(juce::Slider::LinearVertical);
    pitchSlider.setRange(-16.0, 16.0, 0.1);
    pitchSlider.setValue(0.0, juce::dontSendNotification);
    pitchSlider.setTextBoxStyle(juce::Slider::TextBoxBelow, false, 48, 18);
    pitchSlider.setTextValueSuffix("%");
    pitchSlider.setColour(juce::Slider::trackColourId, accent);
    pitchSlider.onValueChange = [this]
    {
        params.tempo_percent = static_cast<float>(pitchSlider.getValue());
        pushParams();
    };
    addAndMakeVisible(pitchSlider);

    for (size_t i = 0; i < hotCueBtns.size(); ++i)
    {
        hotCueBtns[i].setButtonText("HC" + juce::String(static_cast<int>(i + 1)));
        hotCueBtns[i].onClick = [this, i]
        {
            bool shiftHeld = juce::ModifierKeys::getCurrentModifiers().isShiftDown();
            chronomix_dj_hot_cue(bridge.raw(), deckIndex, static_cast<uint32_t>(i), shiftHeld ? 1 : 0);
        };
        addAndMakeVisible(hotCueBtns[i]);
    }

    loopBeatsBox.addItem("1/2 Beat", 1);
    loopBeatsBox.addItem("1 Beat", 2);
    loopBeatsBox.addItem("2 Beats", 3);
    loopBeatsBox.addItem("4 Beats", 4);
    loopBeatsBox.addItem("8 Beats", 5);
    loopBeatsBox.addItem("16 Beats", 6);
    loopBeatsBox.setSelectedId(4, juce::dontSendNotification);
    addAndMakeVisible(loopBeatsBox);

    loopToggleBtn.setClickingTogglesState(true);
    loopToggleBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::green);
    loopToggleBtn.onClick = [this]
    {
        static const float beatTable[] = { 0.5f, 1.0f, 2.0f, 4.0f, 8.0f, 16.0f };
        int idx = juce::jlimit(0, 5, loopBeatsBox.getSelectedId() - 1);
        chronomix_dj_set_loop(bridge.raw(), deckIndex, loopToggleBtn.getToggleState() ? 1 : 0, beatTable[idx]);
    };
    addAndMakeVisible(loopToggleBtn);
}

void DjDeckComponent::pushParams()
{
    CDjDeckParams cur {};
    chronomix_dj_get_deck_params(bridge.raw(), deckIndex, &cur);
    cur.tempo_percent = params.tempo_percent;
    chronomix_dj_set_deck_params(bridge.raw(), deckIndex, &cur);
}

bool DjDeckComponent::loadTrackFile(const juce::File& file)
{
    juce::String err;
    bool ok = bridge.loadDjDeckFile(deckIndex, file, err);
    if (!ok)
    {
        if (onStatusOrError)
            onStatusOrError("Error cargando pista DJ: " + err);
        return false;
    }
    refreshFromEngine();
    if (onStatusOrError)
        onStatusOrError("Cargado en " + juce::String(deckIndex == 0 ? "Deck A: " : "Deck B: ") + trackTitle);
    return true;
}

void DjDeckComponent::refreshFromEngine()
{
    trackTitle = ChronomixBridge::takeString(chronomix_dj_get_deck_title(bridge.raw(), deckIndex));
    if (trackTitle.isEmpty())
        titleLabel.setText(deckIndex == 0 ? "DECK A — Sin pista" : "DECK B — Sin pista", juce::dontSendNotification);
    else
        titleLabel.setText(juce::String(deckIndex == 0 ? "A: " : "B: ") + trackTitle, juce::dontSendNotification);

    std::vector<CWaveformBin> bins(512);
    uint32_t count = chronomix_dj_get_waveform(bridge.raw(), deckIndex, bins.data(), 512);
    bins.resize(count);
    waveformView.setWaveform(bins);

    chronomix_dj_get_deck_params(bridge.raw(), deckIndex, &params);
    pitchSlider.setValue(params.tempo_percent, juce::dontSendNotification);
    loopToggleBtn.setToggleState(params.loop_enabled != 0, juce::dontSendNotification);
}

void DjDeckComponent::updateTelemetry(const CDjDeckTelemetry& t)
{
    tel = t;
    waveformView.setTelemetry(t);
    jogWheel.setTelemetry(t);

    double curSec = (t.sample_rate > 0) ? static_cast<double>(t.pos_frames) / static_cast<double>(t.sample_rate) : 0.0;
    double durSec = (t.sample_rate > 0) ? static_cast<double>(t.total_frames) / static_cast<double>(t.sample_rate) : 0.0;
    timeLabel.setText(formatTimeMmSs(curSec) + " / " + formatTimeMmSs(durSec), juce::dontSendNotification);
    bpmLabel.setText(juce::String(t.effective_bpm, 1) + " BPM", juce::dontSendNotification);

    playBtn.setToggleState(t.playing != 0, juce::dontSendNotification);
    playBtn.setButtonText(t.playing != 0 ? "PAUSE" : "PLAY");
    loopToggleBtn.setToggleState(t.loop_enabled != 0, juce::dontSendNotification);

    for (size_t i = 0; i < hotCueBtns.size(); ++i)
    {
        bool set = t.hot_cue_set[i] != 0;
        hotCueBtns[i].setColour(juce::TextButton::buttonColourId,
                                set ? StudioColors::yellow.withAlpha(0.35f) : StudioColors::bgCard);
    }
}

bool DjDeckComponent::isInterestedInFileDrag(const juce::StringArray&)
{
    dragHover = true;
    repaint();
    return true;
}

void DjDeckComponent::filesDropped(const juce::StringArray& files, int, int)
{
    dragHover = false;
    repaint();
    if (!files.isEmpty())
        loadTrackFile(juce::File(files[0]));
}

void DjDeckComponent::paint(juce::Graphics& g)
{
    auto b = getLocalBounds().toFloat();
    g.setColour(StudioColors::bgBezel);
    g.fillRect(b);
    g.setColour(dragHover ? accent : StudioColors::borderSubtle);
    g.drawRect(b, dragHover ? 2.0f : 1.0f);
    g.setColour(accent);
    g.fillRect(b.getX(), b.getY(), b.getWidth(), 3.0f);
}

void DjDeckComponent::resized()
{
    auto area = getLocalBounds().reduced(8);
    area.removeFromTop(3);

    auto topRow = area.removeFromTop(26);
    ejectBtn.setBounds(topRow.removeFromRight(26));
    topRow.removeFromRight(4);
    loadBtn.setBounds(topRow.removeFromRight(102));
    topRow.removeFromRight(6);
    titleLabel.setBounds(topRow);

    area.removeFromTop(4);
    auto metaRow = area.removeFromTop(20);
    bpmLabel.setBounds(metaRow.removeFromRight(88));
    timeLabel.setBounds(metaRow);

    area.removeFromTop(5);
    int wfH = juce::jlimit(68, 118, area.getHeight() / 4);
    waveformView.setBounds(area.removeFromTop(wfH));

    area.removeFromTop(6);
    auto bottomPads = area.removeFromBottom(60);
    auto hcRow = bottomPads.removeFromTop(26);
    int hcW = (hcRow.getWidth() - 12) / 4;
    for (size_t i = 0; i < hotCueBtns.size(); ++i)
    {
        hotCueBtns[i].setBounds(hcRow.removeFromLeft(hcW));
        hcRow.removeFromLeft(4);
    }
    bottomPads.removeFromTop(6);
    auto loopRow = bottomPads.removeFromTop(26);
    loopToggleBtn.setBounds(loopRow.removeFromRight(76));
    loopRow.removeFromRight(4);
    loopBeatsBox.setBounds(loopRow);

    area.removeFromBottom(6);
    auto transportRow = area.removeFromBottom(30);
    int btnW = (transportRow.getWidth() - 12) / 4;
    playBtn.setBounds(transportRow.removeFromLeft(btnW));
    transportRow.removeFromLeft(4);
    cueBtn.setBounds(transportRow.removeFromLeft(btnW));
    transportRow.removeFromLeft(4);
    setCueBtn.setBounds(transportRow.removeFromLeft(btnW));
    transportRow.removeFromLeft(4);
    syncBtn.setBounds(transportRow);

    area.removeFromBottom(6);
    auto pitchCol = area.removeFromRight(54);
    pitchResetBtn.setBounds(pitchCol.removeFromBottom(22));
    pitchCol.removeFromBottom(2);
    pitchSlider.setBounds(pitchCol);

    area.removeFromRight(6);
    jogWheel.setBounds(area);
}

DjPanel::DjPanel(ChronomixBridge& bridgeRef)
    : bridge(bridgeRef),
      deckA(bridgeRef, 0, StudioColors::cyan),
      deckB(bridgeRef, 1, StudioColors::orange)
{
    chronomix_dj_get_deck_params(bridge.raw(), 0, &paramsA);
    chronomix_dj_get_deck_params(bridge.raw(), 1, &paramsB);
    chronomix_dj_get_mixer_params(bridge.raw(), &mixerParams);

    deckA.onStatusOrError = [this](const juce::String& m) { if (onStatusOrError) onStatusOrError(m); };
    deckB.onStatusOrError = [this](const juce::String& m) { if (onStatusOrError) onStatusOrError(m); };
    addAndMakeVisible(deckA);
    addAndMakeVisible(deckB);

    setupStrip(stripA, 0, "CH A", StudioColors::cyan);
    setupStrip(stripB, 1, "CH B", StudioColors::orange);

    masterKnob.setSliderStyle(juce::Slider::RotaryHorizontalVerticalDrag);
    masterKnob.setRange(0.0, 1.5, 0.01);
    masterKnob.setValue(mixerParams.master_volume, juce::dontSendNotification);
    masterKnob.setTextBoxStyle(juce::Slider::TextBoxBelow, false, 48, 16);
    masterKnob.setColour(juce::Slider::rotarySliderFillColourId, StudioColors::orange);
    masterKnob.onValueChange = [this]
    {
        mixerParams.master_volume = static_cast<float>(masterKnob.getValue());
        pushMixerParams();
    };
    addAndMakeVisible(masterKnob);

    crossfader.setSliderStyle(juce::Slider::LinearHorizontal);
    crossfader.setRange(-1.0, 1.0, 0.005);
    crossfader.setValue(mixerParams.crossfader, juce::dontSendNotification);
    crossfader.setTextBoxStyle(juce::Slider::NoTextBox, false, 0, 0);
    crossfader.setColour(juce::Slider::trackColourId, StudioColors::orange);
    crossfader.onValueChange = [this]
    {
        mixerParams.crossfader = static_cast<float>(crossfader.getValue());
        pushMixerParams();
    };
    addAndMakeVisible(crossfader);

    curveBox.addItem("Curva: Smooth Power", 1);
    curveBox.addItem("Curva: Lineal", 2);
    curveBox.addItem("Curva: Scratch Cut", 3);
    curveBox.setSelectedId(static_cast<int>(mixerParams.curve) + 1, juce::dontSendNotification);
    curveBox.onChange = [this]
    {
        mixerParams.curve = static_cast<uint8_t>(juce::jlimit(0, 2, curveBox.getSelectedId() - 1));
        pushMixerParams();
    };
    addAndMakeVisible(curveBox);

    autoFadeABtn.onClick = [this]
    {
        chronomix_dj_start_auto_crossfade(bridge.raw(), -1.0f, 3.0f);
    };
    addAndMakeVisible(autoFadeABtn);

    autoFadeBBtn.onClick = [this]
    {
        chronomix_dj_start_auto_crossfade(bridge.raw(), 1.0f, 3.0f);
    };
    addAndMakeVisible(autoFadeBBtn);
}

void DjPanel::setupStrip(MixerStripControls& strip, uint32_t deck, const juce::String& name, juce::Colour col)
{
    auto initKnob = [&](juce::Slider& s, double minV, double maxV, double defV)
    {
        s.setSliderStyle(juce::Slider::RotaryHorizontalVerticalDrag);
        s.setRange(minV, maxV, 0.1);
        s.setValue(defV, juce::dontSendNotification);
        s.setDoubleClickReturnValue(true, defV);
        s.setTextBoxStyle(juce::Slider::NoTextBox, false, 0, 0);
        s.setColour(juce::Slider::rotarySliderFillColourId, col);
        s.onValueChange = [this, deck] { pushDeckStripParams(deck); };
        addAndMakeVisible(s);
    };

    strip.headerLabel.setText(name, juce::dontSendNotification);
    strip.headerLabel.setFont(juce::FontOptions(11.5f, juce::Font::bold));
    strip.headerLabel.setColour(juce::Label::textColourId, col);
    strip.headerLabel.setJustificationType(juce::Justification::centred);
    addAndMakeVisible(strip.headerLabel);

    initKnob(strip.trimKnob, -18.0, 12.0, 0.0);
    initKnob(strip.eqHigh, -24.0, 12.0, 0.0);
    initKnob(strip.eqMid, -24.0, 12.0, 0.0);
    initKnob(strip.eqLow, -24.0, 12.0, 0.0);
    initKnob(strip.filterKnob, -1.0, 1.0, 0.0);
    strip.filterKnob.setRange(-1.0, 1.0, 0.01);

    auto initKill = [&](juce::TextButton& b)
    {
        b.setClickingTogglesState(true);
        b.setColour(juce::TextButton::buttonOnColourId, StudioColors::red);
        b.onClick = [this, deck] { pushDeckStripParams(deck); };
        addAndMakeVisible(b);
    };
    initKill(strip.killHigh);
    initKill(strip.killMid);
    initKill(strip.killLow);

    strip.volFader.setSliderStyle(juce::Slider::LinearVertical);
    strip.volFader.setRange(0.0, 1.0, 0.01);
    strip.volFader.setValue(0.85, juce::dontSendNotification);
    strip.volFader.setTextBoxStyle(juce::Slider::NoTextBox, false, 0, 0);
    strip.volFader.setColour(juce::Slider::trackColourId, col);
    strip.volFader.onValueChange = [this, deck] { pushDeckStripParams(deck); };
    addAndMakeVisible(strip.volFader);
}

void DjPanel::pushDeckStripParams(uint32_t deck)
{
    auto& strip = (deck == 0) ? stripA : stripB;
    auto& p = (deck == 0) ? paramsA : paramsB;
    chronomix_dj_get_deck_params(bridge.raw(), deck, &p);
    p.gain_db = static_cast<float>(strip.trimKnob.getValue());
    p.eq_high_db = static_cast<float>(strip.eqHigh.getValue());
    p.eq_mid_db = static_cast<float>(strip.eqMid.getValue());
    p.eq_low_db = static_cast<float>(strip.eqLow.getValue());
    p.kill_high = strip.killHigh.getToggleState() ? 1 : 0;
    p.kill_mid = strip.killMid.getToggleState() ? 1 : 0;
    p.kill_low = strip.killLow.getToggleState() ? 1 : 0;
    p.filter = static_cast<float>(strip.filterKnob.getValue());
    p.volume = static_cast<float>(strip.volFader.getValue());
    chronomix_dj_set_deck_params(bridge.raw(), deck, &p);
}

void DjPanel::pushMixerParams()
{
    chronomix_dj_set_mixer_params(bridge.raw(), &mixerParams);
}

bool DjPanel::loadTrackOnDeck(uint32_t deck, const juce::File& file)
{
    return (deck == 0) ? deckA.loadTrackFile(file) : deckB.loadTrackFile(file);
}

void DjPanel::updateTelemetry()
{
    chronomix_dj_get_telemetry(bridge.raw(), &lastTel);
    deckA.updateTelemetry(lastTel.decks[0]);
    deckB.updateTelemetry(lastTel.decks[1]);
    if (!crossfader.isMouseButtonDown())
    {
        if (std::abs(crossfader.getValue() - static_cast<double>(lastTel.crossfader)) > 0.002)
            crossfader.setValue(lastTel.crossfader, juce::dontSendNotification);
    }
    repaint();
}

void DjPanel::paint(juce::Graphics& g)
{
    g.fillAll(StudioColors::bgDeep);
    auto midArea = getLocalBounds().reduced(6);
    int deckW = juce::jmax(220, (midArea.getWidth() - 252) / 2);
    auto mixerRect = midArea.withTrimmedLeft(deckW + 6).withTrimmedRight(deckW + 6).toFloat();
    g.setColour(StudioColors::bgBezel);
    g.fillRect(mixerRect);
    g.setColour(StudioColors::borderSubtle);
    g.drawRect(mixerRect, 1.0f);

    ChronomixLookAndFeel::drawSegmentedMeter(g, meterARect, juce::jmax(lastTel.decks[0].peak_l, lastTel.decks[0].peak_r), true, 16);
    ChronomixLookAndFeel::drawSegmentedMeter(g, meterMasterRect, juce::jmax(lastTel.master_peak_l, lastTel.master_peak_r), true, 16);
    ChronomixLookAndFeel::drawSegmentedMeter(g, meterBRect, juce::jmax(lastTel.decks[1].peak_l, lastTel.decks[1].peak_r), true, 16);
}

void DjPanel::resized()
{
    auto area = getLocalBounds().reduced(6);
    int mixerW = juce::jlimit(220, 280, area.getWidth() / 4);
    int deckW = juce::jmax(180, (area.getWidth() - mixerW - 12) / 2);

    deckA.setBounds(area.removeFromLeft(deckW));
    area.removeFromLeft(6);
    auto mixerArea = area.removeFromLeft(mixerW).reduced(6);
    area.removeFromLeft(6);
    deckB.setBounds(area);

    auto xfSection = mixerArea.removeFromBottom(86);
    auto autoRow = xfSection.removeFromBottom(24);
    autoFadeABtn.setBounds(autoRow.removeFromLeft(autoRow.getWidth() / 2 - 2));
    autoRow.removeFromLeft(4);
    autoFadeBBtn.setBounds(autoRow);
    xfSection.removeFromBottom(4);
    curveBox.setBounds(xfSection.removeFromBottom(22));
    xfSection.removeFromBottom(4);
    crossfader.setBounds(xfSection);

    mixerArea.removeFromBottom(6);
    auto faderRow = mixerArea.removeFromBottom(juce::jlimit(85, 145, mixerArea.getHeight() / 3));
    int stripW = mixerArea.getWidth() / 2 - 2;
    auto faderLeft = faderRow.removeFromLeft(stripW);
    faderRow.removeFromLeft(4);
    auto faderRight = faderRow;

    stripA.volFader.setBounds(faderLeft.removeFromLeft(faderLeft.getWidth() - 18));
    meterARect = faderLeft.reduced(2, 4).toFloat();

    meterBRect = faderRight.removeFromLeft(18).reduced(2, 4).toFloat();
    stripB.volFader.setBounds(faderRight);

    auto topMaster = mixerArea.removeFromTop(54);
    masterKnob.setBounds(topMaster.withSizeKeepingCentre(54, 52));
    meterMasterRect = topMaster.removeFromRight(14).reduced(1, 2).toFloat();

    auto cols = mixerArea;
    auto colA = cols.removeFromLeft(cols.getWidth() / 2 - 2);
    cols.removeFromLeft(4);
    auto colB = cols;

    auto layoutEqCol = [](MixerStripControls& s, juce::Rectangle<int> c)
    {
        s.headerLabel.setBounds(c.removeFromTop(18));
        int rowH = juce::jmax(28, c.getHeight() / 5);
        s.trimKnob.setBounds(c.removeFromTop(rowH).withSizeKeepingCentre(rowH, rowH));

        auto rHi = c.removeFromTop(rowH);
        s.killHigh.setBounds(rHi.removeFromRight(22).reduced(1, 4));
        s.eqHigh.setBounds(rHi.withSizeKeepingCentre(rowH, rowH));

        auto rMid = c.removeFromTop(rowH);
        s.killMid.setBounds(rMid.removeFromRight(22).reduced(1, 4));
        s.eqMid.setBounds(rMid.withSizeKeepingCentre(rowH, rowH));

        auto rLo = c.removeFromTop(rowH);
        s.killLow.setBounds(rLo.removeFromRight(22).reduced(1, 4));
        s.eqLow.setBounds(rLo.withSizeKeepingCentre(rowH, rowH));

        s.filterKnob.setBounds(c.withSizeKeepingCentre(juce::jmin(rowH, c.getHeight()), juce::jmin(rowH, c.getHeight())));
    };

    layoutEqCol(stripA, colA);
    layoutEqCol(stripB, colB);
}
