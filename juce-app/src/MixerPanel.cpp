#include "MixerPanel.h"
#include <cmath>

static const uint32_t kSwatchPalette[16] = {
    0xff14161c, 0xff363a46, 0xff969cac, 0xffe6ebf5,
    0xffeb3038, 0xfff5731e, 0xfff5af23, 0xfff5d72d,
    0xff8cd72d, 0xff24c369, 0xff20cdaf, 0xff1cc3f0,
    0xff3091f5, 0xff3c5feb, 0xff9b4bf0, 0xffeb37a0
};

void ConsoleTabCanvas::paint(juce::Graphics& g)
{
    g.fillAll(juce::Colour(0xff0b0e14));

    for (const auto& s : sections)
    {
        auto rf = s.bounds.toFloat();
        g.setColour(juce::Colour(0xff11151f));
        g.fillRect(rf);

        auto hdr = s.bounds.withHeight(19);
        g.setColour(juce::Colour(0xff171c28));
        g.fillRect(hdr);

        g.setColour(s.accent);
        g.fillRect(hdr.getX(), hdr.getY(), 3, hdr.getHeight());

        g.setColour(StudioColors::borderSubtle);
        g.drawRect(rf, 1.0f);
        g.drawHorizontalLine(hdr.getBottom(), rf.getX(), rf.getRight());

        g.setColour(s.accent);
        g.setFont(juce::FontOptions(9.5f, juce::Font::bold));
        g.drawText(s.title, hdr.withTrimmedLeft(8).withTrimmedRight(6), juce::Justification::centredLeft);
    }

    for (const auto& lbl : labels)
    {
        g.setColour(lbl.colour);
        g.setFont(juce::FontOptions(9.5f, juce::Font::bold));
        g.drawFittedText(lbl.text, lbl.bounds, lbl.align, 1);
    }
}

EqCurveDisplay::EqCurveDisplay(ChronomixBridge& b) : bridge(b) {}

void EqCurveDisplay::setChannel(uint32_t chIdx, bool enabled, const std::array<float, 4>& freqs,
                                const std::array<float, 4>& gains, const std::array<bool, 4>& bandEn)
{
    ch = chIdx;
    eqEnabled = enabled;
    bandFreqs = freqs;
    bandGains = gains;
    bandEnabled = bandEn;
    repaint();
}

void EqCurveDisplay::paint(juce::Graphics& g)
{
    auto rect = getLocalBounds().toFloat();
    g.setColour(juce::Colour(0xff070a10));
    g.fillRect(rect);
    g.setColour(StudioColors::borderSubtle);
    g.drawRect(rect, 1.0f);

    const float minF = std::log(20.0f);
    const float maxF = std::log(20000.0f);
    const float midY = rect.getCentreY();

    for (float hz : { 50.0f, 100.0f, 200.0f, 500.0f, 1000.0f, 2000.0f, 5000.0f, 10000.0f })
    {
        float t = (std::log(hz) - minF) / (maxF - minF);
        float x = rect.getX() + t * rect.getWidth();
        g.setColour(juce::Colour(0xff151c29));
        g.drawVerticalLine(static_cast<int>(x), rect.getY() + 2.0f, rect.getBottom() - 2.0f);
    }

    for (float db : { -12.0f, -6.0f, 0.0f, 6.0f, 12.0f })
    {
        float y = midY - (db / 20.0f) * (rect.getHeight() * 0.45f);
        g.setColour(std::abs(db) < 0.01f ? StudioColors::borderBright : juce::Colour(0xff151c29));
        g.drawHorizontalLine(static_cast<int>(y), rect.getX() + 2.0f, rect.getRight() - 2.0f);
    }

    juce::Path curve;
    constexpr int steps = 96;
    for (int i = 0; i <= steps; ++i)
    {
        float t = static_cast<float>(i) / static_cast<float>(steps);
        float freq = std::exp(minF + t * (maxF - minF));
        float db = juce::jlimit(-20.0f, 20.0f, chronomix_eval_channel_eq_db(bridge.raw(), ch, freq));
        float x = rect.getX() + t * rect.getWidth();
        float y = juce::jlimit(rect.getY() + 2.0f, rect.getBottom() - 2.0f,
                               midY - (db / 20.0f) * (rect.getHeight() * 0.45f));
        if (i == 0)
            curve.startNewSubPath(x, y);
        else
            curve.lineTo(x, y);

        g.setColour(StudioColors::orange.withAlpha(0.15f));
        g.drawLine(x, juce::jmin(y, midY), x, juce::jmax(y, midY), 1.5f);
    }
    g.setColour(StudioColors::orange);
    g.strokePath(curve, juce::PathStrokeType(2.0f));

    float maxBoost = 0.0f;
    if (eqEnabled)
    {
        for (size_t i = 0; i < 4; ++i)
        {
            if (!bandEnabled[i])
                continue;
            maxBoost = juce::jmax(maxBoost, bandGains[i]);
            float t = (std::log(juce::jlimit(20.0f, 20000.0f, bandFreqs[i])) - minF) / (maxF - minF);
            float x = rect.getX() + t * rect.getWidth();
            float y = juce::jlimit(rect.getY() + 5.0f, rect.getBottom() - 5.0f,
                                   midY - (juce::jlimit(-18.0f, 18.0f, bandGains[i]) / 20.0f) * (rect.getHeight() * 0.45f));
            auto dotCol = (bandGains[i] > 7.0f) ? StudioColors::red
                        : ((bandGains[i] > 3.0f) ? StudioColors::yellow : StudioColors::orange);
            g.setColour(dotCol);
            g.fillEllipse(x - 4.0f, y - 4.0f, 8.0f, 8.0f);
            g.setColour(juce::Colours::white);
            g.setFont(juce::FontOptions(9.0f));
            g.drawText(juce::String(static_cast<int>(i + 1)), static_cast<int>(x - 6), static_cast<int>(y - 16), 12, 12, juce::Justification::centred);
        }
    }

    juce::String badgeText = "NOMINAL";
    juce::Colour badgeCol = StudioColors::green;
    if (!eqEnabled)
    {
        badgeText = "EQ BYPASS";
        badgeCol = StudioColors::textMuted;
    }
    else if (maxBoost > 7.0f)
    {
        badgeText = "SATURACION (+" + juce::String(maxBoost, 1) + " dB)";
        badgeCol = StudioColors::red;
    }
    else if (maxBoost > 3.0f)
    {
        badgeText = "NIVEL ALTO (+" + juce::String(maxBoost, 1) + " dB)";
        badgeCol = StudioColors::yellow;
    }

    auto badgeRect = juce::Rectangle<float>(rect.getRight() - 144.0f, rect.getY() + 4.0f, 140.0f, 16.0f);
    g.setColour(juce::Colour(0xff0b0e14));
    g.fillRect(badgeRect);
    g.setColour(badgeCol);
    g.drawRect(badgeRect, 1.0f);
    g.setFont(juce::FontOptions(9.0f, juce::Font::bold));
    g.drawText(badgeText, badgeRect.toNearestInt(), juce::Justification::centred);
}

void DynamicsCurveDisplay::setParams(bool gateOn, float gateThr, bool compOn, float compThr, float compRatio, float grDb)
{
    gateEnabled = gateOn;
    gateThreshold = gateThr;
    compEnabled = compOn;
    compThreshold = compThr;
    ratio = compRatio;
    gainReductionDb = grDb;
    repaint();
}

void DynamicsCurveDisplay::paint(juce::Graphics& g)
{
    auto r = getLocalBounds().toFloat();
    g.setColour(juce::Colour(0xff070a10));
    g.fillRect(r);
    g.setColour(StudioColors::borderSubtle);
    g.drawRect(r, 1.0f);

    auto graph = r.reduced(6.0f);
    auto grBar = graph.removeFromRight(14.0f);
    graph.removeFromRight(4.0f);

    g.setColour(juce::Colour(0xff1c212e));
    g.drawLine(graph.getX(), graph.getBottom(), graph.getRight(), graph.getY(), 1.0f);

    juce::Path transfer;
    for (int i = 0; i <= 40; ++i)
    {
        float inDb = -60.0f + (static_cast<float>(i) / 40.0f) * 60.0f;
        float outDb = inDb;
        if (gateEnabled && inDb < gateThreshold)
            outDb = -60.0f;
        if (compEnabled && outDb > compThreshold)
            outDb = compThreshold + (outDb - compThreshold) / juce::jmax(1.0f, ratio);
        float x = graph.getX() + ((inDb + 60.0f) / 60.0f) * graph.getWidth();
        float y = graph.getBottom() - ((juce::jlimit(-60.0f, 0.0f, outDb) + 60.0f) / 60.0f) * graph.getHeight();
        if (i == 0) transfer.startNewSubPath(x, y);
        else transfer.lineTo(x, y);
    }
    g.setColour(StudioColors::orange);
    g.strokePath(transfer, juce::PathStrokeType(2.0f));

    g.setColour(juce::Colour(0xff0c0e14));
    g.fillRect(grBar);
    float grFrac = juce::jlimit(0.0f, 1.0f, std::abs(gainReductionDb) / 24.0f);
    g.setColour(StudioColors::red);
    g.fillRect(grBar.getX() + 1.0f, grBar.getY() + 1.0f, grBar.getWidth() - 2.0f, grFrac * (grBar.getHeight() - 2.0f));
    g.setColour(StudioColors::borderSubtle);
    g.drawRect(grBar, 1.0f);
}

void EqBandSatMeter::paint(juce::Graphics& g)
{
    auto b = getLocalBounds().toFloat();
    auto bar = b.removeFromTop(10.0f);
    ChronomixLookAndFeel::drawEqSaturationBar(g, bar, gainDb, enabled);

    juce::String txt = !enabled ? "BYPASS" : (gainDb > 7.0f ? "SATURACION" : (gainDb > 3.0f ? "ALTO" : "NOMINAL"));
    juce::Colour col = !enabled ? StudioColors::textMuted
                     : (gainDb > 7.0f ? StudioColors::red
                     : (gainDb > 3.0f ? StudioColors::yellow : StudioColors::green));
    g.setColour(col);
    g.setFont(juce::FontOptions(9.0f, juce::Font::bold));
    g.drawText(txt, b.toNearestInt(), juce::Justification::centredLeft);
}

void ChannelMiniMeter::paint(juce::Graphics& g)
{
    auto r = getLocalBounds().toFloat();
    g.setColour(juce::Colour(0xff070a10));
    g.fillRect(r);
    g.setColour(StudioColors::borderSubtle);
    g.drawRect(r, 1.0f);

    auto inner = r.reduced(5.0f, 3.0f);
    float rowH = inner.getHeight() / 3.0f;

    auto drawRow = [&](int idx, const juce::String& label, float val, bool isGr)
    {
        auto row = juce::Rectangle<float>(inner.getX(), inner.getY() + static_cast<float>(idx) * rowH, inner.getWidth(), rowH - 2.0f);
        auto lblR = row.removeFromLeft(28.0f);
        g.setColour(isGr ? StudioColors::orange : StudioColors::textMuted);
        g.setFont(juce::FontOptions(8.5f, juce::Font::bold));
        g.drawText(label, lblR.toNearestInt(), juce::Justification::centredLeft);

        if (isGr)
        {
            g.setColour(juce::Colour(0xff0e1118));
            g.fillRect(row);
            g.setColour(StudioColors::borderSubtle);
            g.drawRect(row, 1.0f);
            float frac = juce::jlimit(0.0f, 1.0f, std::abs(val) / 24.0f);
            float fillW = frac * (row.getWidth() - 2.0f);
            g.setColour(StudioColors::orange);
            g.fillRect(row.getRight() - 1.0f - fillW, row.getY() + 1.0f, fillW, row.getHeight() - 2.0f);
        }
        else
        {
            ChronomixLookAndFeel::drawSegmentedMeter(g, row, val, false, 18);
        }
    };

    drawRow(0, "IN", juce::jmax(inLeft, inRight), false);
    drawRow(1, "OUT", juce::jmax(outLeft, outRight), false);
    drawRow(2, "GR", compGrDb, true);
}

MixerPanel::ChannelStripWidget::ChannelStripWidget()
{
    fader.setSliderStyle(juce::Slider::LinearVertical);
    fader.setRange(-60.0, 10.0, 0.1);
    fader.setValue(0.0, juce::dontSendNotification);
    fader.setDoubleClickReturnValue(true, 0.0);
    fader.setTextBoxStyle(juce::Slider::TextBoxBelow, false, 64, 16);
    fader.setTextValueSuffix(" dB");
    fader.onValueChange = [this] { if (onFader) onFader(chIdx, static_cast<float>(fader.getValue())); };
    addAndMakeVisible(fader);

    muteBtn.setClickingTogglesState(true);
    muteBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::red);
    muteBtn.onClick = [this] { if (onToggleMute) onToggleMute(chIdx); };
    addAndMakeVisible(muteBtn);

    soloBtn.setClickingTogglesState(true);
    soloBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::yellow);
    soloBtn.onClick = [this] { if (onToggleSolo) onToggleSolo(chIdx); };
    addAndMakeVisible(soloBtn);

    recBtn.setClickingTogglesState(true);
    recBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::red);
    recBtn.onClick = [this] { if (onToggleRec) onToggleRec(chIdx); };
    addAndMakeVisible(recBtn);

    fxBtn.onClick = [this] { if (onOpenFx) onOpenFx(chIdx); };
    addAndMakeVisible(fxBtn);
}

void MixerPanel::ChannelStripWidget::mouseDown(const juce::MouseEvent&)
{
    if (onSelect)
        onSelect(chIdx);
}

void MixerPanel::ChannelStripWidget::paint(juce::Graphics& g)
{
    auto b = getLocalBounds().toFloat();
    g.setColour(selected ? StudioColors::bgStripSel : StudioColors::bgStrip);
    g.fillRect(b);
    g.setColour(selected ? StudioColors::orange : StudioColors::borderSubtle);
    g.drawRect(b, selected ? 1.5f : 1.0f);

    auto scribble = b.reduced(4.0f).removeFromTop(42.0f);
    g.setColour(juce::Colour(0xff090b10));
    g.fillRect(scribble);
    g.setColour(chColour);
    g.fillRect(scribble.getX(), scribble.getY(), scribble.getWidth(), 7.0f);
    g.setColour(selected ? StudioColors::orange : StudioColors::borderSubtle);
    g.drawRect(scribble, 1.0f);

    g.setColour(StudioColors::textPrimary);
    g.setFont(juce::FontOptions(10.0f, juce::Font::bold));
    g.drawFittedText(chName,
                     juce::Rectangle<int>(static_cast<int>(scribble.getX()) + 2, static_cast<int>(scribble.getY()) + 8,
                                          static_cast<int>(scribble.getWidth()) - 4, 16),
                     juce::Justification::centred, 1);

    auto badgeCol = isMediaChannel ? StudioColors::orange
                  : (hasInputAssigned ? StudioColors::cyan : StudioColors::textMuted);
    g.setColour(badgeCol);
    g.setFont(juce::FontOptions(8.5f, juce::Font::bold));
    g.drawFittedText(chSourceBadge,
                     juce::Rectangle<int>(static_cast<int>(scribble.getX()) + 2, static_cast<int>(scribble.getY()) + 25,
                                          static_cast<int>(scribble.getWidth()) - 4, 14),
                     juce::Justification::centred, 1);

    auto meterArea = juce::Rectangle<float>(b.getRight() - 20.0f, 92.0f, 15.0f, juce::jmax(36.0f, b.getHeight() - 114.0f));
    auto ml = meterArea.removeFromLeft(7.0f);
    meterArea.removeFromLeft(1.0f);
    auto mr = meterArea;
    ChronomixLookAndFeel::drawSegmentedMeter(g, ml, meterL, true, 12);
    ChronomixLookAndFeel::drawSegmentedMeter(g, mr, meterR, true, 12);
}

void MixerPanel::ChannelStripWidget::resized()
{
    auto a = getLocalBounds().reduced(4);
    a.removeFromTop(46);
    auto row1 = a.removeFromTop(20);
    muteBtn.setBounds(row1.removeFromLeft(row1.getWidth() / 2 - 1));
    row1.removeFromLeft(2);
    soloBtn.setBounds(row1);
    a.removeFromTop(2);
    auto row2 = a.removeFromTop(20);
    recBtn.setBounds(row2.removeFromLeft(row2.getWidth() / 2 - 1));
    row2.removeFromLeft(2);
    fxBtn.setBounds(row2);
    a.removeFromTop(4);
    a.removeFromRight(20);
    fader.setBounds(a);
}

int MixerPanel::modeValueToComboId(int modeVal)
{
    switch (modeVal)
    {
        case 1: return 2;
        case 2: return 3;
        case 3: return 4;
        case 4: return 5;
        case 6: return 6;
        case 7: return 7;
        case 5: return 8;
        default: return 1;
    }
}

uint8_t MixerPanel::comboIdToModeValue(int comboId)
{
    switch (comboId)
    {
        case 2: return 1;
        case 3: return 2;
        case 4: return 3;
        case 5: return 4;
        case 6: return 6;
        case 7: return 7;
        case 8: return 5;
        default: return 0;
    }
}

juce::String MixerPanel::modeShortLabel(int modeVal)
{
    switch (modeVal)
    {
        case 1: return "ST 1+2";
        case 2: return "MONO 1";
        case 3: return "MONO 2";
        case 4: return "ST 3+4";
        case 5: return "MEDIA";
        case 6: return "MONO 3";
        case 7: return "MONO 4";
        default: return "---";
    }
}

void MixerPanel::populateInputModeBox(juce::ComboBox& box)
{
    box.clear(juce::dontSendNotification);
    box.addItem("Sin asignar", 1);
    box.addItem("Estereo (In 1+2)", 2);
    box.addItem("Mono Izq (In 1)", 3);
    box.addItem("Mono Der (In 2)", 4);
    box.addItem("Estereo (In 3+4)", 5);
    box.addItem("Mono (In 3)", 6);
    box.addItem("Mono (In 4)", 7);
    box.addItem("Multimedia / Pista", 8);
}

void MixerPanel::populateInputDeviceBox(juce::ComboBox& box, const juce::String& selectedDevId)
{
    box.clear(juce::dontSendNotification);
    box.addItem("-- Sin entrada fisica --", 1);
    int matchedId = 1;
    if (auto* ins = cachedDevices["inputs"].getArray())
    {
        for (int i = 0; i < ins->size(); ++i)
        {
            auto dev = (*ins)[i];
            auto name = dev["name"].toString();
            int chCount = static_cast<int>(dev["channels"]);
            juce::String label = name;
            if (chCount > 0)
                label += " (" + juce::String(chCount) + "ch)";
            box.addItem(label, i + 2);
            if (selectedDevId.isNotEmpty() && dev["id"].toString() == selectedDevId)
                matchedId = i + 2;
        }
    }
    box.setSelectedId(matchedId, juce::dontSendNotification);
}

void MixerPanel::refreshAudioDevices()
{
    bridge.refreshDevices();
    cachedDevices = bridge.listDevices();
    refreshState();
    auto* ins = cachedDevices["inputs"].getArray();
    auto* outs = cachedDevices["outputs"].getArray();
    int nIn = ins ? ins->size() : 0;
    int nOut = outs ? outs->size() : 0;
    notifyMsg("Dispositivos de audio actualizados: " + juce::String(nIn) + " entradas, " + juce::String(nOut) + " salidas");
}

void MixerPanel::applyInputAssignmentFromBoxes(juce::ComboBox& devBox, juce::ComboBox& modeBox, bool changedDevice)
{
    if (updatingControls)
        return;
    if (masterSelected)
    {
        notifyMsg("Selecciona un canal (CH 01..48) para asignar una entrada de audio.");
        refreshState();
        return;
    }

    auto* inputs = cachedDevices["inputs"].getArray();
    int devItemId = devBox.getSelectedId();
    int modeItemId = modeBox.getSelectedId();
    uint8_t mode = comboIdToModeValue(modeItemId);

    if (changedDevice)
    {
        if (devItemId <= 1)
        {
            bridge.setChannelInput(selectedChannel, "", "", 0);
            refreshState();
            notifyMsg(bridge.getStatusText());
            return;
        }
        int devIdx = devItemId - 2;
        if (inputs == nullptr || devIdx < 0 || devIdx >= inputs->size())
        {
            notifyMsg("Dispositivo de entrada no disponible. Pulsa Refrescar.");
            refreshState();
            return;
        }
        auto d = (*inputs)[devIdx];
        if (mode == 0 || mode == 5)
        {
            int chCount = static_cast<int>(d["channels"]);
            mode = (chCount == 1) ? 2 : 1;
        }
        if (!bridge.setChannelInput(selectedChannel, d["id"].toString(), d["name"].toString(), mode))
        {
            auto err = bridge.takeLastError();
            if (err.isNotEmpty())
                notifyMsg(err);
        }
        else
        {
            notifyMsg(bridge.getStatusText());
        }
        refreshState();
        return;
    }

    if (mode == 5)
    {
        chronomix_set_channel_multimedia(bridge.raw(), selectedChannel);
        refreshState();
        notifyMsg("CH " + juce::String(selectedChannel + 1) + ": modo Multimedia");
        return;
    }
    if (mode == 0)
    {
        bridge.setChannelInput(selectedChannel, "", "", 0);
        refreshState();
        notifyMsg(bridge.getStatusText());
        return;
    }

    int devIdx = devItemId - 2;
    if (inputs == nullptr || inputs->isEmpty())
    {
        notifyMsg("Error: no se detectaron dispositivos de entrada de audio. Pulsa Refrescar.");
        refreshState();
        return;
    }
    if (devIdx < 0 || devIdx >= inputs->size())
    {
        auto defIn = cachedDevices["default_input_id"].toString();
        devIdx = 0;
        for (int i = 0; i < inputs->size(); ++i)
        {
            if ((*inputs)[i]["id"].toString() == defIn)
            {
                devIdx = i;
                break;
            }
        }
    }
    auto d = (*inputs)[devIdx];
    if (!bridge.setChannelInput(selectedChannel, d["id"].toString(), d["name"].toString(), mode))
    {
        auto err = bridge.takeLastError();
        if (err.isNotEmpty())
            notifyMsg(err);
    }
    else
    {
        notifyMsg(bridge.getStatusText());
    }
    refreshState();
}

void MixerPanel::applyOutputSelection()
{
    if (updatingControls)
        return;
    int idx = ioOutDeviceBox.getSelectedItemIndex();
    juce::String outId;
    if (auto* outs = cachedDevices["outputs"].getArray())
        if (idx >= 0 && idx < outs->size())
            outId = (*outs)[idx]["id"].toString();

    uint32_t sr = static_cast<uint32_t>(juce::jmax(44100, ioSampleRateBox.getSelectedId()));
    uint32_t bs = static_cast<uint32_t>(juce::jmax(128, ioBlockSizeBox.getSelectedId()));
    if (!bridge.setOutputDevice(outId, sr, bs))
    {
        auto err = bridge.takeLastError();
        if (err.isNotEmpty())
            notifyMsg(err);
    }
    else
    {
        notifyMsg(bridge.getStatusText());
    }
    refreshState();
}

MixerPanel::MixerPanel(ChronomixBridge& bridgeRef)
    : bridge(bridgeRef),
      homeEqCurve(bridgeRef),
      eqMainCurve(bridgeRef)
{
    cachedPresets = ChronomixBridge::takeJson(chronomix_get_presets_json());
    cachedDevices = bridge.listDevices();
    cachedPlugins = ChronomixBridge::takeJson(chronomix_scan_plugins_json(bridge.raw()));
    cachedWineStatus = ChronomixBridge::takeJson(chronomix_wine_status_json(bridge.raw()));

    const char* leftLabels[9] = {
        "INICIO", "FUENTE", "EQ", "DINAMICA", "PERFILES", "VST FX", "E/S", "AJUSTES", "INFO"
    };
    for (size_t i = 0; i < 9; ++i)
    {
        leftMenuBtns[i].setButtonText(leftLabels[i]);
        leftMenuBtns[i].setClickingTogglesState(false);
        leftMenuBtns[i].setColour(juce::TextButton::buttonColourId, juce::Colour(0xff151822));
        leftMenuBtns[i].setColour(juce::TextButton::buttonOnColourId, juce::Colour(0xff2a1c11));
        leftMenuBtns[i].setColour(juce::TextButton::textColourOffId, StudioColors::textSecond);
        leftMenuBtns[i].setColour(juce::TextButton::textColourOnId, StudioColors::orange);
        leftMenuBtns[i].onClick = [this, i] { setActiveTab(static_cast<MixerTab>(i)); };
        addAndMakeVisible(leftMenuBtns[i]);
    }

    prevChBtn.setColour(juce::TextButton::buttonColourId, juce::Colour(0xff181e2c));
    prevChBtn.onClick = [this]
    {
        auto* arr = cachedState["channels"].getArray();
        int n = arr ? arr->size() : 0;
        if (n <= 0) return;
        if (masterSelected) { masterSelected = false; selectedChannel = static_cast<uint32_t>(n - 1); }
        else if (selectedChannel == 0) masterSelected = true;
        else --selectedChannel;
        populateSelectedChannelControls();
        rebuildChannelStrips();
        repaint();
    };
    addAndMakeVisible(prevChBtn);

    nextChBtn.setColour(juce::TextButton::buttonColourId, juce::Colour(0xff181e2c));
    nextChBtn.onClick = [this]
    {
        auto* arr = cachedState["channels"].getArray();
        int n = arr ? arr->size() : 0;
        if (n <= 0) return;
        if (masterSelected) { masterSelected = false; selectedChannel = 0; }
        else if (static_cast<int>(selectedChannel) + 1 >= n) masterSelected = true;
        else ++selectedChannel;
        populateSelectedChannelControls();
        rebuildChannelStrips();
        repaint();
    };
    addAndMakeVisible(nextChBtn);

    lcdInQuickBtn.setColour(juce::TextButton::buttonColourId, juce::Colour(0xff101824));
    lcdInQuickBtn.setColour(juce::TextButton::textColourOffId, StudioColors::cyan);
    lcdInQuickBtn.onClick = [this] { setActiveTab(MixerTab::Source); };
    addAndMakeVisible(lcdInQuickBtn);

    lcdOutQuickBtn.setColour(juce::TextButton::buttonColourId, juce::Colour(0xff1e1610));
    lcdOutQuickBtn.setColour(juce::TextButton::textColourOffId, StudioColors::orange);
    lcdOutQuickBtn.onClick = [this] { setActiveTab(MixerTab::IoRec); };
    addAndMakeVisible(lcdOutQuickBtn);

    tabViewport.setViewedComponent(&tabContent, false);
    tabViewport.setScrollBarsShown(true, false);
    addAndMakeVisible(tabViewport);

    auto initRotary = [&](juce::Slider& s, double minV, double maxV, double step, double defV, const juce::String& suf)
    {
        s.setSliderStyle(juce::Slider::RotaryHorizontalVerticalDrag);
        s.setRange(minV, maxV, step);
        s.setValue(defV, juce::dontSendNotification);
        s.setDoubleClickReturnValue(true, defV);
        s.setTextValueSuffix(suf);
        s.setTextBoxStyle(juce::Slider::TextBoxBelow, false, 60, 15);
        s.setColour(juce::Slider::rotarySliderFillColourId, StudioColors::orange);
        s.onValueChange = [this] { if (!updatingControls) pushSelectedDspChanges(); };
        tabContent.addChildComponent(s);
    };

    auto initHSlider = [&](juce::Slider& s, double minV, double maxV, double defV, const juce::String& suf, bool pushDsp)
    {
        s.setSliderStyle(juce::Slider::LinearHorizontal);
        s.setRange(minV, maxV, 0.1);
        s.setValue(defV, juce::dontSendNotification);
        s.setDoubleClickReturnValue(true, defV);
        s.setTextValueSuffix(suf);
        s.setTextBoxStyle(juce::Slider::TextBoxRight, false, 62, 20);
        if (pushDsp)
            s.onValueChange = [this] { if (!updatingControls) pushSelectedDspChanges(); };
        tabContent.addChildComponent(s);
    };

    auto initToggle = [&](juce::ToggleButton& tb)
    {
        tb.onClick = [this] { if (!updatingControls) pushSelectedDspChanges(); };
        tabContent.addChildComponent(tb);
    };

    initRotary(homeTrimSlider, -24.0, 24.0, 0.1, 0.0, " dB");
    initRotary(homePanSlider, -1.0, 1.0, 0.01, 0.0, "");
    initRotary(homeBalanceSlider, -1.0, 1.0, 0.01, 0.0, "");
    initRotary(homeWidthSlider, 0.0, 200.0, 1.0, 100.0, "%");
    initToggle(homePhaseToggle);
    initToggle(homeHpfToggle);
    initToggle(homeLpfToggle);
    initToggle(homeEqToggle);
    initToggle(homeGateToggle);
    initToggle(homeCompToggle);
    tabContent.addChildComponent(homeEqCurve);
    tabContent.addChildComponent(homeMiniMeter);

    homeBrowseProfilesBtn.setColour(juce::TextButton::buttonColourId, StudioColors::orange);
    homeBrowseProfilesBtn.setColour(juce::TextButton::textColourOffId, juce::Colours::black);
    homeBrowseProfilesBtn.onClick = [this] { setActiveTab(MixerTab::Profiles); };
    tabContent.addChildComponent(homeBrowseProfilesBtn);

    homeLoadProfileBtn.onClick = [this]
    {
        auto paths = bridge.getPaths();
        fileChooser = std::make_unique<juce::FileChooser>("Cargar Perfil DSP (.json)", juce::File(paths["profiles_dir"].toString()), "*.json");
        fileChooser->launchAsync(juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles,
                                 [this](const juce::FileChooser& fc)
                                 {
                                     auto f = fc.getResult();
                                     if (f.existsAsFile() && !masterSelected)
                                     {
                                         if (chronomix_load_channel_profile_file(bridge.raw(), selectedChannel, f.getFullPathName().toRawUTF8()) != 0)
                                         {
                                             refreshState();
                                             notifyMsg("Perfil DSP cargado: " + f.getFileName());
                                         }
                                     }
                                 });
    };
    tabContent.addChildComponent(homeLoadProfileBtn);

    homeSaveProfileBtn.onClick = [this]
    {
        auto paths = bridge.getPaths();
        fileChooser = std::make_unique<juce::FileChooser>("Guardar Perfil DSP (.json)", juce::File(paths["profiles_dir"].toString()).getChildFile("perfil.json"), "*.json");
        fileChooser->launchAsync(juce::FileBrowserComponent::saveMode | juce::FileBrowserComponent::canSelectFiles,
                                 [this](const juce::FileChooser& fc)
                                 {
                                     auto f = fc.getResult();
                                     if (f != juce::File() && !masterSelected)
                                     {
                                         if (chronomix_save_channel_profile_file(bridge.raw(), selectedChannel, f.getFullPathName().toRawUTF8()) != 0)
                                             notifyMsg("Perfil DSP guardado en " + f.getFullPathName());
                                     }
                                 });
    };
    tabContent.addChildComponent(homeSaveProfileBtn);

    srcNameEditor.onTextChange = [this]
    {
        if (!updatingControls && !masterSelected)
        {
            chronomix_set_channel_name(bridge.raw(), selectedChannel, srcNameEditor.getText().toRawUTF8());
            refreshState();
        }
    };
    tabContent.addChildComponent(srcNameEditor);

    for (size_t i = 0; i < 16; ++i)
    {
        auto col = juce::Colour(kSwatchPalette[i]);
        srcColorBtns[i].setColour(juce::TextButton::buttonColourId, col);
        srcColorBtns[i].onClick = [this, col]
        {
            if (!masterSelected)
            {
                chronomix_set_channel_color(bridge.raw(), selectedChannel, col.getRed(), col.getGreen(), col.getBlue());
                refreshState();
            }
        };
        tabContent.addChildComponent(srcColorBtns[i]);
    }

    populateInputModeBox(srcModeBox);
    srcDeviceBox.onChange = [this] { applyInputAssignmentFromBoxes(srcDeviceBox, srcModeBox, true); };
    srcModeBox.onChange = [this] { applyInputAssignmentFromBoxes(srcDeviceBox, srcModeBox, false); };
    tabContent.addChildComponent(srcDeviceBox);
    tabContent.addChildComponent(srcModeBox);

    srcRefreshDevBtn.onClick = [this] { refreshAudioDevices(); };
    tabContent.addChildComponent(srcRefreshDevBtn);

    srcInputStatusLabel.setFont(juce::FontOptions(11.0f, juce::Font::bold));
    srcInputStatusLabel.setColour(juce::Label::textColourId, StudioColors::cyan);
    tabContent.addChildComponent(srcInputStatusLabel);

    srcLoadMediaBtn.setColour(juce::TextButton::buttonColourId, StudioColors::orange.withAlpha(0.30f));
    srcLoadMediaBtn.onClick = [this]
    {
        if (masterSelected) return;
        fileChooser = std::make_unique<juce::FileChooser>("Cargar Audio en Canal", juce::File(), bridge.getSupportedAudioWildcard());
        fileChooser->launchAsync(juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles,
                                 [this](const juce::FileChooser& fc)
                                 {
                                     auto f = fc.getResult();
                                     if (f.existsAsFile())
                                     {
                                         juce::String err;
                                         if (bridge.loadMixerMediaFile(selectedChannel, f, err))
                                         {
                                             refreshState();
                                             notifyMsg("Audio cargado en CH " + juce::String(selectedChannel + 1));
                                         }
                                         else
                                         {
                                             notifyMsg("Error cargando audio: " + err);
                                         }
                                     }
                                 });
    };
    tabContent.addChildComponent(srcLoadMediaBtn);

    srcMediaPlayBtn.onClick = [this]
    {
        if (masterSelected) return;
        auto chVar = getSelectedChannelVar();
        bool playing = static_cast<bool>(chVar["media"]["playing"]);
        bool loop = srcMediaLoopToggle.getToggleState();
        chronomix_set_media_transport(bridge.raw(), selectedChannel, playing ? 0 : 1, loop ? 1 : 0, -1);
        refreshState();
    };
    tabContent.addChildComponent(srcMediaPlayBtn);

    srcMediaStopBtn.onClick = [this]
    {
        if (masterSelected) return;
        chronomix_set_media_transport(bridge.raw(), selectedChannel, 0, srcMediaLoopToggle.getToggleState() ? 1 : 0, 0);
        refreshState();
    };
    tabContent.addChildComponent(srcMediaStopBtn);

    srcMediaLoopToggle.setToggleState(true, juce::dontSendNotification);
    tabContent.addChildComponent(srcMediaLoopToggle);
    srcMediaInfoLabel.setColour(juce::Label::textColourId, StudioColors::textSecond);
    tabContent.addChildComponent(srcMediaInfoLabel);

    initToggle(eqOnToggle);
    initToggle(eqHpfToggle);
    initHSlider(eqHpfSlider, 20.0, 400.0, 80.0, " Hz", true);
    initToggle(eqLpfToggle);
    initHSlider(eqLpfSlider, 1500.0, 20000.0, 18000.0, " Hz", true);
    eqOpenProfilesBtn.onClick = [this] { setActiveTab(MixerTab::Profiles); };
    tabContent.addChildComponent(eqOpenProfilesBtn);
    eqResetFlatBtn.onClick = [this]
    {
        if (!masterSelected)
        {
            chronomix_apply_preset(bridge.raw(), selectedChannel, 22);
            refreshState();
        }
    };
    tabContent.addChildComponent(eqResetFlatBtn);
    tabContent.addChildComponent(eqMainCurve);

    const char* bandTitles[4] = { "1: LOW SHELF", "2: LO-MID BELL", "3: HI-MID BELL", "4: HIGH SHELF" };
    for (size_t i = 0; i < 4; ++i)
    {
        eqBands[i].enabledToggle.setButtonText(bandTitles[i]);
        initToggle(eqBands[i].enabledToggle);
        initHSlider(eqBands[i].freqSlider, 25.0, 18000.0, 250.0, " Hz", true);
        eqBands[i].freqSlider.setSkewFactorFromMidPoint(1000.0);
        initHSlider(eqBands[i].gainSlider, -18.0, 18.0, 0.0, " dB", true);
        initHSlider(eqBands[i].qSlider, 0.4, 8.0, 1.0, " Q", true);
        eqBands[i].qSlider.setRange(0.4, 8.0, 0.05);
        tabContent.addChildComponent(eqBands[i].satMeter);
    }

    tabContent.addChildComponent(dynCurve);
    initToggle(dynGateToggle);
    initHSlider(dynGateThr, -80.0, -6.0, -48.0, " dB", true);
    initToggle(dynCompToggle);
    initHSlider(dynCompThr, -50.0, 0.0, -18.0, " dB", true);
    initHSlider(dynCompRatio, 1.0, 12.0, 3.0, ":1", true);
    initHSlider(dynCompMakeup, 0.0, 18.0, 0.0, " dB", true);
    initToggle(dynDeessToggle);
    initHSlider(dynDeessFreq, 3000.0, 10000.0, 6500.0, " Hz", true);
    initHSlider(dynDeessThr, -40.0, -6.0, -24.0, " dB", true);
    initToggle(dynDriveToggle);
    initHSlider(dynDriveAmt, 0.0, 100.0, 15.0, "%", true);
    initToggle(dynDelayToggle);
    initHSlider(dynDelayTime, 20.0, 1000.0, 250.0, " ms", true);
    initHSlider(dynDelayFb, 0.0, 85.0, 30.0, "%", true);
    initHSlider(dynDelayMix, 0.0, 100.0, 20.0, "%", true);
    initToggle(dynRevToggle);
    initHSlider(dynRevRoom, 10.0, 95.0, 50.0, "%", true);
    initHSlider(dynRevDamp, 0.0, 80.0, 40.0, "%", true);
    initHSlider(dynRevMix, 0.0, 100.0, 20.0, "%", true);
    initToggle(dynLimToggle);
    initHSlider(dynLimCeil, -6.0, 0.0, -0.3, " dB", true);

    profLoadJsonBtn.onClick = homeLoadProfileBtn.onClick;
    profSaveJsonBtn.onClick = homeSaveProfileBtn.onClick;
    profResetFlatBtn.onClick = eqResetFlatBtn.onClick;
    tabContent.addChildComponent(profLoadJsonBtn);
    tabContent.addChildComponent(profSaveJsonBtn);
    tabContent.addChildComponent(profResetFlatBtn);

    for (size_t i = 0; i < 4; ++i)
    {
        vstSlotBtns[i].setButtonText("SLOT " + juce::String(static_cast<int>(i + 1)));
        vstSlotBtns[i].onClick = [this, i]
        {
            focusedVstSlot = static_cast<uint32_t>(i);
            populateSelectedChannelControls();
        };
        tabContent.addChildComponent(vstSlotBtns[i]);
    }
    vstWineWarningLabel.setColour(juce::Label::textColourId, StudioColors::yellow);
    vstWineWarningLabel.setColour(juce::Label::backgroundColourId, juce::Colour(0xff2c2210));
    vstWineWarningLabel.setFont(juce::FontOptions(11.0f, juce::Font::bold));
    tabContent.addChildComponent(vstWineWarningLabel);

    vstScanBtn.onClick = [this]
    {
        cachedPlugins = ChronomixBridge::takeJson(chronomix_scan_plugins_json(bridge.raw()));
        populateSelectedChannelControls();
        notifyMsg(bridge.getStatusText());
    };
    tabContent.addChildComponent(vstScanBtn);

    auto loadPluginAndWarn = [this](const juce::String& path)
    {
        if (path.endsWithIgnoreCase(".dll") || path.endsWithIgnoreCase(".vst3"))
        {
            auto warn = bridge.getWineWarning();
            if (warn.isNotEmpty() && path.endsWithIgnoreCase(".dll"))
            {
                juce::AlertWindow::showMessageBoxAsync(
                    juce::MessageBoxIconType::WarningIcon,
                    "Puente Wine (wineg++) no disponible",
                    warn);
            }
        }
        int ok = chronomix_load_insert_plugin(
            bridge.raw(),
            masterSelected ? 1 : 0,
            selectedChannel,
            focusedVstSlot,
            path.toRawUTF8());
        if (ok == 0)
        {
            auto err = bridge.takeLastError();
            if (err.isNotEmpty())
            {
                notifyMsg(err);
                if (err.containsIgnoreCase("wineg++"))
                {
                    juce::AlertWindow::showMessageBoxAsync(
                        juce::MessageBoxIconType::WarningIcon,
                        "Falta wineg++ para plugins Windows",
                        err);
                }
            }
        }
        else
        {
            notifyMsg(bridge.getStatusText());
        }
        refreshState();
    };

    vstBrowseBtn.onClick = [this, loadPluginAndWarn]
    {
        auto paths = bridge.getPaths();
        fileChooser = std::make_unique<juce::FileChooser>(
            "Seleccionar Plugin VST2 / VST3",
            juce::File(paths["vst_dir"].toString()),
            "*.vst3;*.so;*.dll");
        fileChooser->launchAsync(juce::FileBrowserComponent::openMode
                                     | juce::FileBrowserComponent::canSelectFiles
                                     | juce::FileBrowserComponent::canSelectDirectories,
                                 [loadPluginAndWarn](const juce::FileChooser& fc)
                                 {
                                     auto f = fc.getResult();
                                     if (f.exists())
                                         loadPluginAndWarn(f.getFullPathName());
                                 });
    };
    tabContent.addChildComponent(vstBrowseBtn);

    vstLoadSelectedBtn.setColour(juce::TextButton::buttonColourId, StudioColors::orange);
    vstLoadSelectedBtn.setColour(juce::TextButton::textColourOffId, juce::Colours::black);
    vstLoadSelectedBtn.onClick = [this, loadPluginAndWarn]
    {
        int idx = vstDiscoveredBox.getSelectedItemIndex();
        auto* arr = cachedPlugins.getArray();
        if (arr != nullptr && idx >= 0 && idx < arr->size())
            loadPluginAndWarn((*arr)[idx]["path"].toString());
    };
    tabContent.addChildComponent(vstLoadSelectedBtn);
    tabContent.addChildComponent(vstDiscoveredBox);

    vstUnloadBtn.onClick = [this]
    {
        chronomix_unload_insert_plugin(bridge.raw(), masterSelected ? 1 : 0, selectedChannel, focusedVstSlot);
        refreshState();
    };
    tabContent.addChildComponent(vstUnloadBtn);

    vstOpenGuiBtn.onClick = [this]
    {
        if (chronomix_open_insert_gui(bridge.raw(), masterSelected ? 1 : 0, selectedChannel, focusedVstSlot) == 0)
        {
            auto err = bridge.takeLastError();
            if (err.isNotEmpty()) notifyMsg(err);
        }
        refreshState();
    };
    tabContent.addChildComponent(vstOpenGuiBtn);

    vstBypassToggle.onClick = [this]
    {
        if (!updatingControls)
        {
            chronomix_set_insert_bypass_mix(bridge.raw(), masterSelected ? 1 : 0, selectedChannel, focusedVstSlot,
                                            vstBypassToggle.getToggleState() ? 1 : 0,
                                            static_cast<float>(vstMixSlider.getValue() * 0.01));
        }
    };
    tabContent.addChildComponent(vstBypassToggle);

    initHSlider(vstMixSlider, 0.0, 100.0, 100.0, "% Mix", false);
    vstMixSlider.onValueChange = [this]
    {
        if (!updatingControls)
        {
            chronomix_set_insert_bypass_mix(bridge.raw(), masterSelected ? 1 : 0, selectedChannel, focusedVstSlot,
                                            vstBypassToggle.getToggleState() ? 1 : 0,
                                            static_cast<float>(vstMixSlider.getValue() * 0.01));
        }
    };

    vstParamFilter.setTextToShowWhenEmpty("Filtrar parametros del plugin...", StudioColors::textMuted);
    vstParamFilter.onTextChange = [this] { rebuildVstParams(); };
    tabContent.addChildComponent(vstParamFilter);

    vstParamsViewport.setViewedComponent(&vstParamsContent, false);
    tabContent.addChildComponent(vstParamsViewport);

    ioOutDeviceBox.onChange = [this] { applyOutputSelection(); };
    ioSampleRateBox.addItem("44100 Hz", 44100);
    ioSampleRateBox.addItem("48000 Hz", 48000);
    ioSampleRateBox.addItem("96000 Hz", 96000);
    ioSampleRateBox.setSelectedId(48000, juce::dontSendNotification);
    ioSampleRateBox.onChange = [this] { applyOutputSelection(); };

    ioBlockSizeBox.addItem("128 muestras", 128);
    ioBlockSizeBox.addItem("256 muestras", 256);
    ioBlockSizeBox.addItem("512 muestras", 512);
    ioBlockSizeBox.addItem("1024 muestras", 1024);
    ioBlockSizeBox.setSelectedId(256, juce::dontSendNotification);
    ioBlockSizeBox.onChange = [this] { applyOutputSelection(); };

    tabContent.addChildComponent(ioOutDeviceBox);
    tabContent.addChildComponent(ioSampleRateBox);
    tabContent.addChildComponent(ioBlockSizeBox);

    populateInputModeBox(ioInModeBox);
    ioInDeviceBox.onChange = [this] { applyInputAssignmentFromBoxes(ioInDeviceBox, ioInModeBox, true); };
    ioInModeBox.onChange = [this] { applyInputAssignmentFromBoxes(ioInDeviceBox, ioInModeBox, false); };
    tabContent.addChildComponent(ioInDeviceBox);
    tabContent.addChildComponent(ioInModeBox);

    ioRoutingStatusLabel.setFont(juce::FontOptions(11.0f, juce::Font::bold));
    ioRoutingStatusLabel.setColour(juce::Label::textColourId, StudioColors::textSecond);
    tabContent.addChildComponent(ioRoutingStatusLabel);

    ioRefreshBtn.onClick = [this] { refreshAudioDevices(); };
    tabContent.addChildComponent(ioRefreshBtn);

    auto toggleLiveAudio = [this]
    {
        if (lastTel.is_audio_running != 0)
        {
            bridge.stopAudio();
            notifyMsg("Motor de audio detenido");
        }
        else
        {
            int idx = ioOutDeviceBox.getSelectedItemIndex();
            juce::String outId;
            if (auto* outs = cachedDevices["outputs"].getArray())
                if (idx >= 0 && idx < outs->size())
                    outId = (*outs)[idx]["id"].toString();
            uint32_t sr = static_cast<uint32_t>(juce::jmax(44100, ioSampleRateBox.getSelectedId()));
            uint32_t bs = static_cast<uint32_t>(juce::jmax(128, ioBlockSizeBox.getSelectedId()));
            if (!bridge.startAudio(outId, sr, bs))
                notifyMsg(bridge.takeLastError());
            else
                notifyMsg(bridge.getStatusText());
        }
        refreshState();
    };
    ioStartStopBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::green);
    ioStartStopBtn.onClick = toggleLiveAudio;
    tabContent.addChildComponent(ioStartStopBtn);

    ioVmicToggle.onClick = [this]
    {
        if (!updatingControls)
        {
            if (chronomix_toggle_virtual_mic(bridge.raw(), ioVmicToggle.getToggleState() ? 1 : 0) == 0)
                notifyMsg(bridge.takeLastError());
            else
                notifyMsg(bridge.getStatusText());
            refreshState();
        }
    };
    tabContent.addChildComponent(ioVmicToggle);

    initHSlider(ioVmicGainSlider, -24.0, 24.0, 0.0, " dB", false);
    ioVmicGainSlider.onValueChange = [this]
    {
        if (!updatingControls)
            chronomix_set_virtual_mic_gain_db(bridge.raw(), static_cast<float>(ioVmicGainSlider.getValue()));
    };

    auto toggleRecording = [this]
    {
        if (lastTel.is_recording != 0)
        {
            auto msg = ChronomixBridge::takeString(chronomix_stop_recording(bridge.raw()));
            if (msg.isNotEmpty()) notifyMsg(msg);
            if (onRefreshLibrary) onRefreshLibrary();
        }
        else
        {
            if (chronomix_start_recording(bridge.raw(), "") == 0)
                notifyMsg(bridge.takeLastError());
            else
                notifyMsg(bridge.getStatusText());
        }
        refreshState();
    };
    ioRecToggleBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::red);
    ioRecToggleBtn.onClick = toggleRecording;
    tabContent.addChildComponent(ioRecToggleBtn);

    ioOpenRecDirBtn.onClick = [this]
    {
        auto p = bridge.getPaths()["recordings_dir"].toString();
        juce::File(p).revealToUser();
    };
    tabContent.addChildComponent(ioOpenRecDirBtn);
    ioRecDirLabel.setColour(juce::Label::textColourId, StudioColors::textSecond);
    tabContent.addChildComponent(ioRecDirLabel);

    setLangBox.addItem("Espanol (ES)", 1);
    setLangBox.addItem("English (EN)", 2);
    setLangBox.setSelectedId(1, juce::dontSendNotification);
    setLangBox.onChange = [this]
    {
        if (updatingControls) return;
        auto s = bridge.getSettings();
        if (auto* obj = s.getDynamicObject())
        {
            obj->setProperty("lang", setLangBox.getSelectedId() == 2 ? "EN" : "ES");
            bridge.updateSettings(s);
        }
    };
    tabContent.addChildComponent(setLangBox);

    setPathsLabel.setJustificationType(juce::Justification::topLeft);
    setPathsLabel.setColour(juce::Label::textColourId, StudioColors::textSecond);
    tabContent.addChildComponent(setPathsLabel);

    setOpenVstDirBtn.onClick = [this]
    {
        juce::File(bridge.getPaths()["vst_dir"].toString()).revealToUser();
    };
    tabContent.addChildComponent(setOpenVstDirBtn);

    setChooseVstDirBtn.onClick = [this]
    {
        fileChooser = std::make_unique<juce::FileChooser>("Elegir carpeta VST personalizada", juce::File(bridge.getPaths()["vst_dir"].toString()));
        fileChooser->launchAsync(juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectDirectories,
                                 [this](const juce::FileChooser& fc)
                                 {
                                     auto f = fc.getResult();
                                     if (f.isDirectory())
                                     {
                                         auto s = bridge.getSettings();
                                         if (auto* obj = s.getDynamicObject())
                                         {
                                             obj->setProperty("custom_vst_dir", f.getFullPathName());
                                             bridge.updateSettings(s);
                                             cachedPlugins = ChronomixBridge::takeJson(chronomix_scan_plugins_json(bridge.raw()));
                                             populateSelectedChannelControls();
                                         }
                                     }
                                 });
    };
    tabContent.addChildComponent(setChooseVstDirBtn);

    setSysVstToggle.onClick = [this]
    {
        if (updatingControls) return;
        auto s = bridge.getSettings();
        if (auto* obj = s.getDynamicObject())
        {
            obj->setProperty("include_system_vst", setSysVstToggle.getToggleState());
            bridge.updateSettings(s);
            cachedPlugins = ChronomixBridge::takeJson(chronomix_scan_plugins_json(bridge.raw()));
            populateSelectedChannelControls();
        }
    };
    tabContent.addChildComponent(setSysVstToggle);

    setWineStatusLabel.setJustificationType(juce::Justification::topLeft);
    setWineStatusLabel.setColour(juce::Label::textColourId, StudioColors::textPrimary);
    tabContent.addChildComponent(setWineStatusLabel);

    setWineRefreshBtn.onClick = [this]
    {
        cachedWineStatus = ChronomixBridge::takeJson(chronomix_wine_status_json(bridge.raw()));
        populateSelectedChannelControls();
    };
    tabContent.addChildComponent(setWineRefreshBtn);

    setWineInitBtn.onClick = [this]
    {
        notifyMsg(ChronomixBridge::takeString(chronomix_wine_init_prefix(bridge.raw())));
        cachedWineStatus = ChronomixBridge::takeJson(chronomix_wine_status_json(bridge.raw()));
        populateSelectedChannelControls();
    };
    tabContent.addChildComponent(setWineInitBtn);

    setWineRebuildBtn.onClick = [this]
    {
        notifyMsg(ChronomixBridge::takeString(chronomix_wine_rebuild_bridge(bridge.raw())));
        cachedWineStatus = ChronomixBridge::takeJson(chronomix_wine_status_json(bridge.raw()));
        populateSelectedChannelControls();
    };
    tabContent.addChildComponent(setWineRebuildBtn);

    setWineCfgBtn.onClick = [this] { notifyMsg(ChronomixBridge::takeString(chronomix_wine_launch_tool(bridge.raw(), 0))); };
    setWineExplorerBtn.onClick = [this] { notifyMsg(ChronomixBridge::takeString(chronomix_wine_launch_tool(bridge.raw(), 1))); };
    tabContent.addChildComponent(setWineCfgBtn);
    tabContent.addChildComponent(setWineExplorerBtn);

    setWineRunExeBtn.onClick = [this]
    {
        fileChooser = std::make_unique<juce::FileChooser>("Ejecutar instalador Windows (.exe / .msi)", juce::File(), "*.exe;*.msi");
        fileChooser->launchAsync(juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectFiles,
                                 [this](const juce::FileChooser& fc)
                                 {
                                     auto f = fc.getResult();
                                     if (f.existsAsFile())
                                         notifyMsg(ChronomixBridge::takeString(chronomix_wine_launch_exe(bridge.raw(), f.getFullPathName().toRawUTF8())));
                                 });
    };
    tabContent.addChildComponent(setWineRunExeBtn);

    newsBodyLabel.setJustificationType(juce::Justification::topLeft);
    newsBodyLabel.setColour(juce::Label::textColourId, StudioColors::textPrimary);
    newsBodyLabel.setText(
        "CHRONOMIX STUDIO DAW v0.0.1 - ARQUITECTURA HIBRIDA JUCE + RUST\n\n"
        "- Interfaz de consola digital con pantalla TFT superior, teclas retroiluminadas, encoders rotatorios y puente de medidores.\n"
        "- Asignacion directa de entradas por canal y salida principal en caliente desde FUENTE y E/S.\n"
        "- Panel DJ de doble bandeja (Deck A + Deck B) con waveforms RGB, jog wheels, BPM Sync, Hot Cues, Auto-Loop e Isolator EQ.\n"
        "- Consola Mixer de hasta 48 canales con EQ de 4 bandas y medidores de saturacion en 3 colores (Verde / Amarillo / Rojo), 23 perfiles DSP e importacion/exportacion JSON.\n"
        "- MIDI Studio con Piano Roll de 24 semitonos x 16 pasos, secuenciador de bateria y teclado tocable.\n"
        "- Almacenamiento estandar XDG y validacion de pantalla multi-monitor.",
        juce::dontSendNotification);
    tabContent.addChildComponent(newsBodyLabel);

    monStartBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::green);
    monStartBtn.onClick = toggleLiveAudio;
    addAndMakeVisible(monStartBtn);

    monRecBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::red);
    monRecBtn.onClick = toggleRecording;
    addAndMakeVisible(monRecBtn);

    monVmicBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::cyan);
    monVmicBtn.onClick = [this]
    {
        bool next = (lastTel.is_virtual_mic_active == 0);
        if (chronomix_toggle_virtual_mic(bridge.raw(), next ? 1 : 0) == 0)
            notifyMsg(bridge.takeLastError());
        else
            notifyMsg(bridge.getStatusText());
        refreshState();
    };
    addAndMakeVisible(monVmicBtn);

    monClearSoloBtn.onClick = [this]
    {
        chronomix_clear_all_solos(bridge.raw());
        refreshState();
    };
    addAndMakeVisible(monClearSoloBtn);

    monPlayAllBtn.onClick = [this]
    {
        chronomix_set_all_media_transport(bridge.raw(), 1);
        refreshState();
    };
    addAndMakeVisible(monPlayAllBtn);

    const char* bankLabels[5] = { "CH 1-16", "CH 17-32", "CH 33-48", "MEDIA", "TODOS" };
    for (size_t i = 0; i < 5; ++i)
    {
        bankBtns[i].setButtonText(bankLabels[i]);
        bankBtns[i].onClick = [this, i]
        {
            activeBank = static_cast<BankFilter>(i);
            rebuildChannelStrips();
        };
        addAndMakeVisible(bankBtns[i]);
    }

    addChBtn.setColour(juce::TextButton::buttonColourId, StudioColors::orange.withAlpha(0.28f));
    addChBtn.onClick = [this]
    {
        int idx = chronomix_add_channel(bridge.raw());
        if (idx >= 0)
        {
            masterSelected = false;
            selectedChannel = static_cast<uint32_t>(idx);
            refreshState();
        }
    };
    addAndMakeVisible(addChBtn);

    addMediaBtn.onClick = [this]
    {
        int idx = chronomix_add_multimedia_channel(bridge.raw());
        if (idx >= 0)
        {
            masterSelected = false;
            selectedChannel = static_cast<uint32_t>(idx);
            activeBank = BankFilter::Multimedia;
            refreshState();
        }
    };
    addAndMakeVisible(addMediaBtn);

    stripsViewport.setViewedComponent(&stripsContent, false);
    stripsViewport.setScrollBarsShown(false, true);
    addAndMakeVisible(stripsViewport);

    masterFader.setSliderStyle(juce::Slider::LinearVertical);
    masterFader.setRange(-60.0, 10.0, 0.1);
    masterFader.setValue(0.0, juce::dontSendNotification);
    masterFader.setDoubleClickReturnValue(true, 0.0);
    masterFader.setTextBoxStyle(juce::Slider::TextBoxBelow, false, 64, 16);
    masterFader.setTextValueSuffix(" dB");
    masterFader.onValueChange = [this]
    {
        if (!updatingControls)
            chronomix_set_master(bridge.raw(), static_cast<float>(masterFader.getValue()),
                                 masterMuteBtn.getToggleState() ? 1 : 0,
                                 masterDimBtn.getToggleState() ? 1 : 0,
                                 masterRecMixBtn.getToggleState() ? 1 : 0);
    };
    addAndMakeVisible(masterFader);

    auto pushMaster = [this]
    {
        if (!updatingControls)
            chronomix_set_master(bridge.raw(), static_cast<float>(masterFader.getValue()),
                                 masterMuteBtn.getToggleState() ? 1 : 0,
                                 masterDimBtn.getToggleState() ? 1 : 0,
                                 masterRecMixBtn.getToggleState() ? 1 : 0);
    };
    masterMuteBtn.setClickingTogglesState(true);
    masterMuteBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::red);
    masterMuteBtn.onClick = pushMaster;
    addAndMakeVisible(masterMuteBtn);

    masterDimBtn.setClickingTogglesState(true);
    masterDimBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::yellow);
    masterDimBtn.onClick = pushMaster;
    addAndMakeVisible(masterDimBtn);

    masterRecMixBtn.setClickingTogglesState(true);
    masterRecMixBtn.setColour(juce::TextButton::buttonOnColourId, StudioColors::red);
    masterRecMixBtn.onClick = pushMaster;
    addAndMakeVisible(masterRecMixBtn);

    masterFxBtn.onClick = [this]
    {
        masterSelected = true;
        setActiveTab(MixerTab::Vst);
        populateSelectedChannelControls();
        rebuildChannelStrips();
    };
    addAndMakeVisible(masterFxBtn);

    rebuildProfileCards();
    refreshState();
    setActiveTab(MixerTab::Home);
}

void MixerPanel::notifyMsg(const juce::String& msg)
{
    if (onNotify && msg.isNotEmpty())
        onNotify(msg);
}

juce::var MixerPanel::getSelectedChannelVar() const
{
    if (auto* arr = cachedState["channels"].getArray())
        if (static_cast<int>(selectedChannel) < arr->size())
            return (*arr)[static_cast<int>(selectedChannel)];
    return {};
}

void MixerPanel::assignMediaFileToNewOrSelectedChannel(const juce::File& file)
{
    int idx = chronomix_add_multimedia_channel(bridge.raw());
    if (idx < 0)
        idx = static_cast<int>(selectedChannel);
    juce::String err;
    if (bridge.loadMixerMediaFile(static_cast<uint32_t>(idx), file, err))
    {
        masterSelected = false;
        selectedChannel = static_cast<uint32_t>(idx);
        activeBank = BankFilter::Multimedia;
        refreshState();
        notifyMsg("Pista asignada a canal CH " + juce::String(idx + 1) + ": " + file.getFileName());
    }
    else
    {
        notifyMsg("Error asignando pista: " + err);
    }
}

void MixerPanel::setActiveTab(MixerTab tab)
{
    activeTab = tab;
    for (size_t i = 0; i < 9; ++i)
        leftMenuBtns[i].setToggleState(static_cast<int>(tab) == static_cast<int>(i), juce::dontSendNotification);

    for (auto* child : tabContent.getChildren())
        child->setVisible(false);

    switch (tab)
    {
        case MixerTab::Home:
            homeTrimSlider.setVisible(true);
            homeBalanceSlider.setVisible(true);
            homePanSlider.setVisible(true);
            homeWidthSlider.setVisible(true);
            homePhaseToggle.setVisible(true);
            homeHpfToggle.setVisible(true);
            homeLpfToggle.setVisible(true);
            homeEqToggle.setVisible(true);
            homeGateToggle.setVisible(true);
            homeCompToggle.setVisible(true);
            homeEqCurve.setVisible(true);
            homeMiniMeter.setVisible(true);
            homeBrowseProfilesBtn.setVisible(true);
            homeLoadProfileBtn.setVisible(true);
            homeSaveProfileBtn.setVisible(true);
            break;
        case MixerTab::Source:
            srcNameEditor.setVisible(true);
            for (auto& b : srcColorBtns) b.setVisible(true);
            srcDeviceBox.setVisible(true);
            srcModeBox.setVisible(true);
            srcRefreshDevBtn.setVisible(true);
            srcInputStatusLabel.setVisible(true);
            srcLoadMediaBtn.setVisible(true);
            srcMediaPlayBtn.setVisible(true);
            srcMediaStopBtn.setVisible(true);
            srcMediaLoopToggle.setVisible(true);
            srcMediaInfoLabel.setVisible(true);
            break;
        case MixerTab::Eq:
            eqOnToggle.setVisible(true);
            eqHpfToggle.setVisible(true);
            eqHpfSlider.setVisible(true);
            eqLpfToggle.setVisible(true);
            eqLpfSlider.setVisible(true);
            eqOpenProfilesBtn.setVisible(true);
            eqResetFlatBtn.setVisible(true);
            eqMainCurve.setVisible(true);
            for (auto& b : eqBands)
            {
                b.enabledToggle.setVisible(true);
                b.freqSlider.setVisible(true);
                b.gainSlider.setVisible(true);
                b.qSlider.setVisible(true);
                b.satMeter.setVisible(true);
            }
            break;
        case MixerTab::Dyn:
            dynCurve.setVisible(true);
            dynGateToggle.setVisible(true); dynGateThr.setVisible(true);
            dynCompToggle.setVisible(true); dynCompThr.setVisible(true); dynCompRatio.setVisible(true); dynCompMakeup.setVisible(true);
            dynDeessToggle.setVisible(true); dynDeessFreq.setVisible(true); dynDeessThr.setVisible(true);
            dynDriveToggle.setVisible(true); dynDriveAmt.setVisible(true);
            dynDelayToggle.setVisible(true); dynDelayTime.setVisible(true); dynDelayFb.setVisible(true); dynDelayMix.setVisible(true);
            dynRevToggle.setVisible(true); dynRevRoom.setVisible(true); dynRevDamp.setVisible(true); dynRevMix.setVisible(true);
            dynLimToggle.setVisible(true); dynLimCeil.setVisible(true);
            break;
        case MixerTab::Profiles:
            profLoadJsonBtn.setVisible(true);
            profSaveJsonBtn.setVisible(true);
            profResetFlatBtn.setVisible(true);
            for (auto& row : profileRows) row->setVisible(true);
            break;
        case MixerTab::Vst:
            for (auto& b : vstSlotBtns) b.setVisible(true);
            vstWineWarningLabel.setVisible(vstWineWarningLabel.getText().isNotEmpty());
            vstScanBtn.setVisible(true);
            vstBrowseBtn.setVisible(true);
            vstDiscoveredBox.setVisible(true);
            vstLoadSelectedBtn.setVisible(true);
            vstUnloadBtn.setVisible(true);
            vstOpenGuiBtn.setVisible(true);
            vstBypassToggle.setVisible(true);
            vstMixSlider.setVisible(true);
            vstParamFilter.setVisible(true);
            vstParamsViewport.setVisible(true);
            break;
        case MixerTab::IoRec:
            ioOutDeviceBox.setVisible(true);
            ioSampleRateBox.setVisible(true);
            ioBlockSizeBox.setVisible(true);
            ioRefreshBtn.setVisible(true);
            ioStartStopBtn.setVisible(true);
            ioInDeviceBox.setVisible(true);
            ioInModeBox.setVisible(true);
            ioRoutingStatusLabel.setVisible(true);
            ioVmicToggle.setVisible(true);
            ioVmicGainSlider.setVisible(true);
            ioRecToggleBtn.setVisible(true);
            ioOpenRecDirBtn.setVisible(true);
            ioRecDirLabel.setVisible(true);
            break;
        case MixerTab::Settings:
            setLangBox.setVisible(true);
            setPathsLabel.setVisible(true);
            setOpenVstDirBtn.setVisible(true);
            setChooseVstDirBtn.setVisible(true);
            setSysVstToggle.setVisible(true);
            setWineStatusLabel.setVisible(true);
            setWineRefreshBtn.setVisible(true);
            setWineInitBtn.setVisible(true);
            setWineRebuildBtn.setVisible(true);
            setWineCfgBtn.setVisible(true);
            setWineExplorerBtn.setVisible(true);
            setWineRunExeBtn.setVisible(true);
            break;
        case MixerTab::News:
            newsBodyLabel.setVisible(true);
            break;
    }

    layoutActiveTabContent(juce::jmax(300, tabViewport.getWidth() - 14));
    repaint();
}

void MixerPanel::refreshState()
{
    cachedState = ChronomixBridge::takeJson(chronomix_get_mixer_state_json(bridge.raw()));
    auto* arr = cachedState["channels"].getArray();
    if (arr != nullptr && !arr->isEmpty() && static_cast<int>(selectedChannel) >= arr->size())
        selectedChannel = static_cast<uint32_t>(arr->size() - 1);
    populateSelectedChannelControls();
    rebuildChannelStrips();
    if (activeTab == MixerTab::IoRec)
        layoutActiveTabContent(juce::jmax(290, tabViewport.getWidth() - 14));
    repaint();
}

void MixerPanel::populateSelectedChannelControls()
{
    updatingControls = true;

    auto m = cachedState["master"];
    masterFader.setValue(static_cast<double>(m["fader_db"]), juce::dontSendNotification);
    masterMuteBtn.setToggleState(static_cast<bool>(m["mute"]), juce::dontSendNotification);
    masterDimBtn.setToggleState(static_cast<bool>(m["dim"]), juce::dontSendNotification);
    masterRecMixBtn.setToggleState(static_cast<bool>(m["record_mix"]), juce::dontSendNotification);
    ioVmicToggle.setToggleState(static_cast<bool>(m["virtual_mic_enabled"]), juce::dontSendNotification);
    ioVmicGainSlider.setValue(static_cast<double>(m["virtual_mic_gain_db"]), juce::dontSendNotification);

    auto activeOutId = cachedState["selected_output_id"].toString();
    if (activeOutId.isEmpty())
        activeOutId = cachedDevices["default_output_id"].toString();

    juce::String activeOutName = cachedState["selected_output_name"].toString();
    ioOutDeviceBox.clear(juce::dontSendNotification);
    if (auto* outs = cachedDevices["outputs"].getArray())
    {
        int selId = outs->isEmpty() ? 0 : 1;
        for (int i = 0; i < outs->size(); ++i)
        {
            auto devName = (*outs)[i]["name"].toString();
            int chCount = static_cast<int>((*outs)[i]["channels"]);
            juce::String label = chCount > 0 ? (devName + " (" + juce::String(chCount) + "ch)") : devName;
            ioOutDeviceBox.addItem(label, i + 1);
            if ((*outs)[i]["id"].toString() == activeOutId)
            {
                selId = i + 1;
                activeOutName = devName;
            }
        }
        if (selId > 0)
            ioOutDeviceBox.setSelectedId(selId, juce::dontSendNotification);
    }

    int prefSr = static_cast<int>(cachedState["preferred_sample_rate"]);
    if (prefSr == 44100 || prefSr == 48000 || prefSr == 96000)
        ioSampleRateBox.setSelectedId(prefSr, juce::dontSendNotification);
    int prefBs = static_cast<int>(cachedState["preferred_block_size"]);
    if (prefBs == 128 || prefBs == 256 || prefBs == 512 || prefBs == 1024)
        ioBlockSizeBox.setSelectedId(prefBs, juce::dontSendNotification);

    juce::String shortOut = activeOutName.isEmpty() ? "SIN SALIDA" : activeOutName.substring(0, 22);
    lcdOutQuickBtn.setButtonText("OUT: " + shortOut);

    vstDiscoveredBox.clear(juce::dontSendNotification);
    if (auto* plugs = cachedPlugins.getArray())
        for (int i = 0; i < plugs->size(); ++i)
            vstDiscoveredBox.addItem((*plugs)[i]["name"].toString() + " [" + (*plugs)[i]["category"].toString() + "]", i + 1);
    if (vstDiscoveredBox.getNumItems() > 0 && vstDiscoveredBox.getSelectedId() == 0)
        vstDiscoveredBox.setSelectedId(1, juce::dontSendNotification);

    auto wineWarn = bridge.getWineWarning();
    vstWineWarningLabel.setText(wineWarn, juce::dontSendNotification);
    vstWineWarningLabel.setVisible(activeTab == MixerTab::Vst && wineWarn.isNotEmpty());

    auto paths = bridge.getPaths();
    auto settings = bridge.getSettings();
    ioRecDirLabel.setText("Grabaciones XDG: " + paths["recordings_dir"].toString(), juce::dontSendNotification);
    setLangBox.setSelectedId(settings["lang"].toString() == "EN" ? 2 : 1, juce::dontSendNotification);
    setSysVstToggle.setToggleState(static_cast<bool>(settings["include_system_vst"]), juce::dontSendNotification);
    setPathsLabel.setText(
        "Configuracion: " + paths["config_dir"].toString() +
        "\nPlugins VST: " + paths["vst_dir"].toString() +
        "\nGrabaciones: " + paths["recordings_dir"].toString() +
        "\nPerfiles DSP: " + paths["profiles_dir"].toString(),
        juce::dontSendNotification);

    juce::String wineTxt =
        "Wine binario: " + (cachedWineStatus["wine_binary"].toString().isEmpty() ? juce::String("No detectado") : cachedWineStatus["wine_binary"].toString()) +
        " (" + cachedWineStatus["wine_version"].toString() + ")\n" +
        "wineg++: " + (cachedWineStatus["winegpp"].toString().isEmpty() ? juce::String("No detectado") : cachedWineStatus["winegpp"].toString()) +
        " | Bridge: " + cachedWineStatus["bridge_mode"].toString() + "\n" +
        "Prefijo: " + cachedWineStatus["prefix"].toString();
    if (wineWarn.isNotEmpty())
        wineTxt += "\n" + wineWarn;
    setWineStatusLabel.setText(wineTxt, juce::dontSendNotification);

    if (masterSelected)
    {
        populateInputDeviceBox(srcDeviceBox, {});
        populateInputDeviceBox(ioInDeviceBox, {});
        srcModeBox.setSelectedId(1, juce::dontSendNotification);
        ioInModeBox.setSelectedId(1, juce::dontSendNotification);
        srcInputStatusLabel.setText("Master Bus seleccionado: elige un canal CH 01..48 para asignar entrada", juce::dontSendNotification);
        ioRoutingStatusLabel.setText("MASTER BUS activo | Salida: " + (activeOutName.isEmpty() ? juce::String("Sin asignar") : activeOutName), juce::dontSendNotification);
        lcdInQuickBtn.setButtonText("IN: MASTER BUS");
    }
    else
    {
        auto ch = getSelectedChannelVar();
        auto chName = ch["name"].toString();
        srcNameEditor.setText(chName, juce::dontSendNotification);

        int mode = static_cast<int>(ch["source_mode"]);
        int modeSel = modeValueToComboId(mode);
        auto devId = ch["source_device_id"].toString();
        auto devLabel = ch["source_device_label"].toString();

        populateInputDeviceBox(srcDeviceBox, devId);
        populateInputDeviceBox(ioInDeviceBox, devId);
        srcModeBox.setSelectedId(modeSel, juce::dontSendNotification);
        ioInModeBox.setSelectedId(modeSel, juce::dontSendNotification);

        juce::String inStatus;
        if (mode == 5)
        {
            inStatus = "CH " + juce::String(selectedChannel + 1) + ": Multimedia / Pista de audio";
            lcdInQuickBtn.setButtonText("IN: MEDIA");
            srcInputStatusLabel.setColour(juce::Label::textColourId, StudioColors::orange);
        }
        else if (mode != 0 && devId.isNotEmpty())
        {
            juce::String shownName = devLabel.isNotEmpty() ? devLabel : devId;
            inStatus = "CONECTADO en CH " + juce::String(selectedChannel + 1) + ": " + shownName + " [" + modeShortLabel(mode) + "]";
            lcdInQuickBtn.setButtonText("IN: " + shownName.substring(0, 18) + " [" + modeShortLabel(mode) + "]");
            srcInputStatusLabel.setColour(juce::Label::textColourId, StudioColors::green);
        }
        else
        {
            inStatus = "CH " + juce::String(selectedChannel + 1) + ": Sin entrada asignada (elige un dispositivo arriba)";
            lcdInQuickBtn.setButtonText("IN: SIN ASIGNAR");
            srcInputStatusLabel.setColour(juce::Label::textColourId, StudioColors::textMuted);
        }
        srcInputStatusLabel.setText(inStatus, juce::dontSendNotification);
        ioRoutingStatusLabel.setText(inStatus, juce::dontSendNotification);

        auto mediaTitle = ch["media"]["title"].toString();
        srcMediaInfoLabel.setText(mediaTitle.isEmpty() ? "Sin archivo multimedia asignado" : ("Pista: " + mediaTitle),
                                  juce::dontSendNotification);
        srcMediaLoopToggle.setToggleState(static_cast<bool>(ch["media"]["looping"]), juce::dontSendNotification);

        auto dsp = ch["dsp"];
        homeTrimSlider.setValue(static_cast<double>(dsp["trim_db"]), juce::dontSendNotification);
        homeBalanceSlider.setValue(static_cast<double>(dsp["balance"]), juce::dontSendNotification);
        homePanSlider.setValue(static_cast<double>(dsp["pan"]), juce::dontSendNotification);
        homeWidthSlider.setValue(static_cast<double>(dsp["stereo_width"]) * 100.0, juce::dontSendNotification);
        homePhaseToggle.setToggleState(static_cast<bool>(dsp["phase_invert"]), juce::dontSendNotification);
        homeHpfToggle.setToggleState(static_cast<bool>(dsp["hpf_enabled"]), juce::dontSendNotification);
        homeLpfToggle.setToggleState(static_cast<bool>(dsp["lpf_enabled"]), juce::dontSendNotification);
        homeEqToggle.setToggleState(static_cast<bool>(dsp["eq_enabled"]), juce::dontSendNotification);
        homeGateToggle.setToggleState(static_cast<bool>(dsp["gate_enabled"]), juce::dontSendNotification);
        homeCompToggle.setToggleState(static_cast<bool>(dsp["comp_enabled"]), juce::dontSendNotification);

        bool eqEn = static_cast<bool>(dsp["eq_enabled"]);
        eqOnToggle.setToggleState(eqEn, juce::dontSendNotification);
        eqHpfToggle.setToggleState(static_cast<bool>(dsp["hpf_enabled"]), juce::dontSendNotification);
        eqHpfSlider.setValue(static_cast<double>(dsp["hpf_freq_hz"]), juce::dontSendNotification);
        eqLpfToggle.setToggleState(static_cast<bool>(dsp["lpf_enabled"]), juce::dontSendNotification);
        eqLpfSlider.setValue(static_cast<double>(dsp["lpf_freq_hz"]), juce::dontSendNotification);

        std::array<float, 4> bFreqs { 100.0f, 500.0f, 2500.0f, 8000.0f };
        std::array<float, 4> bGains { 0.0f, 0.0f, 0.0f, 0.0f };
        std::array<bool, 4> bEn { true, true, true, true };
        if (auto* bands = dsp["eq_bands"].getArray())
        {
            for (int i = 0; i < juce::jmin(4, bands->size()); ++i)
            {
                auto b = (*bands)[i];
                bEn[static_cast<size_t>(i)] = static_cast<bool>(b["enabled"]);
                bFreqs[static_cast<size_t>(i)] = static_cast<float>(static_cast<double>(b["freq_hz"]));
                bGains[static_cast<size_t>(i)] = static_cast<float>(static_cast<double>(b["gain_db"]));
                eqBands[static_cast<size_t>(i)].enabledToggle.setToggleState(bEn[static_cast<size_t>(i)], juce::dontSendNotification);
                eqBands[static_cast<size_t>(i)].freqSlider.setValue(bFreqs[static_cast<size_t>(i)], juce::dontSendNotification);
                eqBands[static_cast<size_t>(i)].gainSlider.setValue(bGains[static_cast<size_t>(i)], juce::dontSendNotification);
                eqBands[static_cast<size_t>(i)].qSlider.setValue(static_cast<double>(b["q"]), juce::dontSendNotification);
                eqBands[static_cast<size_t>(i)].satMeter.setState(bGains[static_cast<size_t>(i)], eqEn && bEn[static_cast<size_t>(i)]);
            }
        }
        homeEqCurve.setChannel(selectedChannel, eqEn, bFreqs, bGains, bEn);
        eqMainCurve.setChannel(selectedChannel, eqEn, bFreqs, bGains, bEn);

        dynGateToggle.setToggleState(static_cast<bool>(dsp["gate_enabled"]), juce::dontSendNotification);
        dynGateThr.setValue(static_cast<double>(dsp["gate_threshold_db"]), juce::dontSendNotification);
        dynCompToggle.setToggleState(static_cast<bool>(dsp["comp_enabled"]), juce::dontSendNotification);
        dynCompThr.setValue(static_cast<double>(dsp["comp_threshold_db"]), juce::dontSendNotification);
        dynCompRatio.setValue(static_cast<double>(dsp["comp_ratio"]), juce::dontSendNotification);
        dynCompMakeup.setValue(static_cast<double>(dsp["comp_makeup_db"]), juce::dontSendNotification);
        dynDeessToggle.setToggleState(static_cast<bool>(dsp["deesser_enabled"]), juce::dontSendNotification);
        dynDeessFreq.setValue(static_cast<double>(dsp["deesser_freq_hz"]), juce::dontSendNotification);
        dynDeessThr.setValue(static_cast<double>(dsp["deesser_threshold_db"]), juce::dontSendNotification);
        dynDriveToggle.setToggleState(static_cast<bool>(dsp["drive_enabled"]), juce::dontSendNotification);
        dynDriveAmt.setValue(static_cast<double>(dsp["drive_amount"]) * 100.0, juce::dontSendNotification);
        dynDelayToggle.setToggleState(static_cast<bool>(dsp["delay_enabled"]), juce::dontSendNotification);
        dynDelayTime.setValue(static_cast<double>(dsp["delay_time_ms"]), juce::dontSendNotification);
        dynDelayFb.setValue(static_cast<double>(dsp["delay_feedback"]) * 100.0, juce::dontSendNotification);
        dynDelayMix.setValue(static_cast<double>(dsp["delay_mix"]) * 100.0, juce::dontSendNotification);
        dynRevToggle.setToggleState(static_cast<bool>(dsp["reverb_enabled"]), juce::dontSendNotification);
        dynRevRoom.setValue(static_cast<double>(dsp["reverb_room_size"]) * 100.0, juce::dontSendNotification);
        dynRevDamp.setValue(static_cast<double>(dsp["reverb_damp"]) * 100.0, juce::dontSendNotification);
        dynRevMix.setValue(static_cast<double>(dsp["reverb_mix"]) * 100.0, juce::dontSendNotification);
        dynLimToggle.setToggleState(static_cast<bool>(dsp["limiter_enabled"]), juce::dontSendNotification);
        dynLimCeil.setValue(static_cast<double>(dsp["limiter_ceiling_db"]), juce::dontSendNotification);

        float gr = (selectedChannel < 48) ? lastTel.channels[selectedChannel].comp_gr_db : 0.0f;
        dynCurve.setParams(dynGateToggle.getToggleState(), static_cast<float>(dynGateThr.getValue()),
                           dynCompToggle.getToggleState(), static_cast<float>(dynCompThr.getValue()),
                           static_cast<float>(dynCompRatio.getValue()), gr);
    }

    auto insertsVar = masterSelected ? cachedState["master"]["inserts"] : getSelectedChannelVar()["inserts"];
    if (auto* slots = insertsVar.getArray())
    {
        for (int i = 0; i < juce::jmin(4, slots->size()); ++i)
        {
            bool loaded = static_cast<bool>((*slots)[i]["loaded"]);
            juce::String nm = loaded ? (*slots)[i]["name"].toString().substring(0, 14) : ("SLOT " + juce::String(i + 1) + " [Vacio]");
            vstSlotBtns[static_cast<size_t>(i)].setButtonText(nm);
            vstSlotBtns[static_cast<size_t>(i)].setToggleState(static_cast<uint32_t>(i) == focusedVstSlot, juce::dontSendNotification);
        }
        if (static_cast<int>(focusedVstSlot) < slots->size())
        {
            auto sl = (*slots)[static_cast<int>(focusedVstSlot)];
            vstBypassToggle.setToggleState(static_cast<bool>(sl["bypass"]), juce::dontSendNotification);
            vstMixSlider.setValue(static_cast<double>(sl["mix"]) * 100.0, juce::dontSendNotification);
        }
    }
    rebuildVstParams();

    updatingControls = false;
}

void MixerPanel::pushSelectedDspChanges()
{
    if (masterSelected) return;
    auto dsp = ChronomixBridge::takeJson(chronomix_get_channel_dsp_json(bridge.raw(), selectedChannel));
    auto* obj = dsp.getDynamicObject();
    if (obj == nullptr) return;

    obj->setProperty("trim_db", homeTrimSlider.getValue());
    obj->setProperty("balance", homeBalanceSlider.getValue());
    obj->setProperty("pan", homePanSlider.getValue());
    obj->setProperty("stereo_width", homeWidthSlider.getValue() * 0.01);
    obj->setProperty("phase_invert", homePhaseToggle.getToggleState());

    bool hpfOn = (activeTab == MixerTab::Eq) ? eqHpfToggle.getToggleState() : homeHpfToggle.getToggleState();
    bool lpfOn = (activeTab == MixerTab::Eq) ? eqLpfToggle.getToggleState() : homeLpfToggle.getToggleState();
    bool eqOn = (activeTab == MixerTab::Home) ? homeEqToggle.getToggleState() : eqOnToggle.getToggleState();
    bool gateOn = (activeTab == MixerTab::Dyn) ? dynGateToggle.getToggleState() : homeGateToggle.getToggleState();
    bool compOn = (activeTab == MixerTab::Dyn) ? dynCompToggle.getToggleState() : homeCompToggle.getToggleState();

    obj->setProperty("hpf_enabled", hpfOn);
    obj->setProperty("hpf_freq_hz", eqHpfSlider.getValue());
    obj->setProperty("lpf_enabled", lpfOn);
    obj->setProperty("lpf_freq_hz", eqLpfSlider.getValue());
    obj->setProperty("eq_enabled", eqOn);

    if (auto* bands = dsp["eq_bands"].getArray())
    {
        std::array<float, 4> bFreqs {};
        std::array<float, 4> bGains {};
        std::array<bool, 4> bEn {};
        for (int i = 0; i < juce::jmin(4, bands->size()); ++i)
        {
            if (auto* bo = (*bands)[i].getDynamicObject())
            {
                bEn[static_cast<size_t>(i)] = eqBands[static_cast<size_t>(i)].enabledToggle.getToggleState();
                bFreqs[static_cast<size_t>(i)] = static_cast<float>(eqBands[static_cast<size_t>(i)].freqSlider.getValue());
                bGains[static_cast<size_t>(i)] = static_cast<float>(eqBands[static_cast<size_t>(i)].gainSlider.getValue());
                bo->setProperty("enabled", bEn[static_cast<size_t>(i)]);
                bo->setProperty("freq_hz", bFreqs[static_cast<size_t>(i)]);
                bo->setProperty("gain_db", bGains[static_cast<size_t>(i)]);
                bo->setProperty("q", eqBands[static_cast<size_t>(i)].qSlider.getValue());
                eqBands[static_cast<size_t>(i)].satMeter.setState(bGains[static_cast<size_t>(i)], eqOn && bEn[static_cast<size_t>(i)]);
            }
        }
        homeEqCurve.setChannel(selectedChannel, eqOn, bFreqs, bGains, bEn);
        eqMainCurve.setChannel(selectedChannel, eqOn, bFreqs, bGains, bEn);
    }

    obj->setProperty("gate_enabled", gateOn);
    obj->setProperty("gate_threshold_db", dynGateThr.getValue());
    obj->setProperty("comp_enabled", compOn);
    obj->setProperty("comp_threshold_db", dynCompThr.getValue());
    obj->setProperty("comp_ratio", dynCompRatio.getValue());
    obj->setProperty("comp_makeup_db", dynCompMakeup.getValue());
    obj->setProperty("deesser_enabled", dynDeessToggle.getToggleState());
    obj->setProperty("deesser_freq_hz", dynDeessFreq.getValue());
    obj->setProperty("deesser_threshold_db", dynDeessThr.getValue());
    obj->setProperty("drive_enabled", dynDriveToggle.getToggleState());
    obj->setProperty("drive_amount", dynDriveAmt.getValue() * 0.01);
    obj->setProperty("delay_enabled", dynDelayToggle.getToggleState());
    obj->setProperty("delay_time_ms", dynDelayTime.getValue());
    obj->setProperty("delay_feedback", dynDelayFb.getValue() * 0.01);
    obj->setProperty("delay_mix", dynDelayMix.getValue() * 0.01);
    obj->setProperty("reverb_enabled", dynRevToggle.getToggleState());
    obj->setProperty("reverb_room_size", dynRevRoom.getValue() * 0.01);
    obj->setProperty("reverb_damp", dynRevDamp.getValue() * 0.01);
    obj->setProperty("reverb_mix", dynRevMix.getValue() * 0.01);
    obj->setProperty("limiter_enabled", dynLimToggle.getToggleState());
    obj->setProperty("limiter_ceiling_db", dynLimCeil.getValue());

    auto json = juce::JSON::toString(dsp, true);
    chronomix_set_channel_dsp_json(bridge.raw(), selectedChannel, json.toRawUTF8());

    homeHpfToggle.setToggleState(hpfOn, juce::dontSendNotification);
    eqHpfToggle.setToggleState(hpfOn, juce::dontSendNotification);
    homeLpfToggle.setToggleState(lpfOn, juce::dontSendNotification);
    eqLpfToggle.setToggleState(lpfOn, juce::dontSendNotification);
    homeEqToggle.setToggleState(eqOn, juce::dontSendNotification);
    eqOnToggle.setToggleState(eqOn, juce::dontSendNotification);
    homeGateToggle.setToggleState(gateOn, juce::dontSendNotification);
    dynGateToggle.setToggleState(gateOn, juce::dontSendNotification);
    homeCompToggle.setToggleState(compOn, juce::dontSendNotification);
    dynCompToggle.setToggleState(compOn, juce::dontSendNotification);

    float gr = (selectedChannel < 48) ? lastTel.channels[selectedChannel].comp_gr_db : 0.0f;
    dynCurve.setParams(gateOn, static_cast<float>(dynGateThr.getValue()),
                       compOn, static_cast<float>(dynCompThr.getValue()),
                       static_cast<float>(dynCompRatio.getValue()), gr);
    repaint();
}

void MixerPanel::rebuildProfileCards()
{
    for (auto& r : profileRows)
        tabContent.removeChildComponent(r.get());
    profileRows.clear();

    auto* arr = cachedPresets.getArray();
    if (arr == nullptr) return;

    struct ProfileCard : public juce::Component
    {
        juce::String name, category, desc, badges;
        juce::TextButton applyBtn { "APLICAR" };
        std::function<void()> onApply;
        ProfileCard()
        {
            applyBtn.setColour(juce::TextButton::buttonColourId, StudioColors::orange);
            applyBtn.setColour(juce::TextButton::textColourOffId, juce::Colours::black);
            applyBtn.onClick = [this] { if (onApply) onApply(); };
            addAndMakeVisible(applyBtn);
        }
        void paint(juce::Graphics& g) override
        {
            auto b = getLocalBounds().toFloat();
            g.setColour(juce::Colour(0xff131824));
            g.fillRect(b);
            g.setColour(StudioColors::borderSubtle);
            g.drawRect(b, 1.0f);
            g.setColour(StudioColors::orange);
            g.fillRect(0.0f, 0.0f, 3.0f, b.getHeight());

            g.setColour(StudioColors::orange);
            g.setFont(juce::FontOptions(9.0f, juce::Font::bold));
            g.drawText(category.toUpperCase(), 10, 4, getWidth() - 102, 12, juce::Justification::centredLeft);

            g.setColour(StudioColors::textPrimary);
            g.setFont(juce::FontOptions(12.0f, juce::Font::bold));
            g.drawText(name, 10, 16, getWidth() - 102, 16, juce::Justification::centredLeft);

            g.setColour(StudioColors::textSecond);
            g.setFont(juce::FontOptions(10.0f));
            g.drawFittedText(desc, 10, 32, getWidth() - 102, 14, juce::Justification::centredLeft, 1);

            g.setColour(StudioColors::cyan);
            g.setFont(juce::FontOptions(9.0f, juce::Font::bold));
            g.drawText(badges, 10, 46, getWidth() - 102, 13, juce::Justification::centredLeft);
        }
        void resized() override
        {
            applyBtn.setBounds(getWidth() - 84, (getHeight() - 26) / 2, 76, 26);
        }
    };

    for (int i = 0; i < arr->size(); ++i)
    {
        auto p = (*arr)[i];
        auto card = std::make_unique<ProfileCard>();
        card->name = p["name"].toString();
        card->category = p["category"].toString();
        card->desc = p["description"].toString();
        juce::StringArray bList;
        if (static_cast<bool>(p["eq"])) bList.add("[EQ]");
        if (static_cast<bool>(p["gate"])) bList.add("[GATE]");
        if (static_cast<bool>(p["comp"])) bList.add("[COMP]");
        if (static_cast<bool>(p["deesser"])) bList.add("[DE-ESS]");
        if (static_cast<bool>(p["drive"])) bList.add("[DRIVE]");
        if (static_cast<bool>(p["delay"])) bList.add("[DELAY]");
        if (static_cast<bool>(p["reverb"])) bList.add("[REVERB]");
        if (static_cast<bool>(p["limiter"])) bList.add("[LIMITER]");
        card->badges = bList.joinIntoString(" ");
        uint32_t presetIdx = static_cast<uint32_t>(static_cast<int>(p["index"]));
        card->onApply = [this, presetIdx]
        {
            if (masterSelected)
            {
                notifyMsg("Selecciona un canal (CH 01..48) para aplicar el perfil DSP.");
                return;
            }
            if (chronomix_apply_preset(bridge.raw(), selectedChannel, presetIdx) != 0)
            {
                refreshState();
                notifyMsg(bridge.getStatusText());
            }
        };
        tabContent.addChildComponent(card.get());
        profileRows.push_back(std::move(card));
    }
}

void MixerPanel::rebuildVstParams()
{
    for (auto& r : vstParamRows)
        vstParamsContent.removeChildComponent(r.get());
    vstParamRows.clear();

    auto insertsVar = masterSelected ? cachedState["master"]["inserts"] : getSelectedChannelVar()["inserts"];
    auto* slots = insertsVar.getArray();
    if (slots == nullptr || static_cast<int>(focusedVstSlot) >= slots->size())
        return;
    auto sl = (*slots)[static_cast<int>(focusedVstSlot)];
    auto* params = sl["params"].getArray();
    if (params == nullptr) return;

    auto filter = vstParamFilter.getText().trim().toLowerCase();

    struct ParamRow : public juce::Component
    {
        juce::Label nameLabel;
        juce::Slider slider;
        juce::Label valLabel;
        ParamRow()
        {
            nameLabel.setFont(juce::FontOptions(11.0f));
            addAndMakeVisible(nameLabel);
            slider.setSliderStyle(juce::Slider::LinearHorizontal);
            slider.setRange(0.0, 1.0, 0.001);
            slider.setTextBoxStyle(juce::Slider::NoTextBox, false, 0, 0);
            addAndMakeVisible(slider);
            valLabel.setFont(juce::FontOptions(10.5f));
            valLabel.setJustificationType(juce::Justification::centredRight);
            addAndMakeVisible(valLabel);
        }
        void resized() override
        {
            auto a = getLocalBounds();
            nameLabel.setBounds(a.removeFromLeft(150));
            valLabel.setBounds(a.removeFromRight(72));
            slider.setBounds(a);
        }
    };

    int y = 2;
    int w = juce::jmax(260, vstParamsViewport.getWidth() - 18);
    for (int i = 0; i < params->size(); ++i)
    {
        auto p = (*params)[i];
        auto nm = p["name"].toString();
        if (filter.isNotEmpty() && !nm.toLowerCase().contains(filter))
            continue;
        auto row = std::make_unique<ParamRow>();
        row->nameLabel.setText(nm, juce::dontSendNotification);
        row->slider.setValue(static_cast<double>(p["value"]), juce::dontSendNotification);
        row->valLabel.setText(p["formatted"].toString(), juce::dontSendNotification);
        uint32_t pid = static_cast<uint32_t>(static_cast<int>(p["id"]));
        auto* rowPtr = row.get();
        row->slider.onValueChange = [this, pid, rowPtr]
        {
            auto fmt = ChronomixBridge::takeString(chronomix_set_insert_param(
                bridge.raw(), masterSelected ? 1 : 0, selectedChannel, focusedVstSlot, pid, rowPtr->slider.getValue()));
            if (fmt.isNotEmpty())
                rowPtr->valLabel.setText(fmt, juce::dontSendNotification);
        };
        row->setBounds(2, y, w, 24);
        vstParamsContent.addAndMakeVisible(row.get());
        vstParamRows.push_back(std::move(row));
        y += 26;
    }
    vstParamsContent.setSize(w + 4, juce::jmax(40, y + 4));
}

void MixerPanel::rebuildChannelStrips()
{
    for (size_t i = 0; i < 5; ++i)
        bankBtns[i].setToggleState(static_cast<int>(activeBank) == static_cast<int>(i), juce::dontSendNotification);

    for (auto& s : channelStrips)
        stripsContent.removeChildComponent(s.get());
    channelStrips.clear();

    auto* arr = cachedState["channels"].getArray();
    if (arr == nullptr) return;

    int x = 4;
    const int stripW = 84;
    const int stripH = juce::jmax(170, stripsViewport.getHeight() - 16);

    for (int i = 0; i < arr->size(); ++i)
    {
        auto ch = (*arr)[i];
        int srcMode = static_cast<int>(ch["source_mode"]);
        bool isMedia = (srcMode == 5);
        bool matches = false;
        switch (activeBank)
        {
            case BankFilter::All: matches = true; break;
            case BankFilter::Multimedia: matches = isMedia; break;
            case BankFilter::Ch1_16: matches = !isMedia && i < 16; break;
            case BankFilter::Ch17_32: matches = !isMedia && i >= 16 && i < 32; break;
            case BankFilter::Ch33_48: matches = !isMedia && i >= 32; break;
        }
        if (!matches) continue;

        auto st = std::make_unique<ChannelStripWidget>();
        st->chIdx = static_cast<uint32_t>(i);
        st->chName = ch["name"].toString();
        st->isMediaChannel = isMedia;
        st->hasInputAssigned = (!isMedia && srcMode != 0 && ch["source_device_id"].toString().isNotEmpty());
        st->chSourceBadge = modeShortLabel(srcMode);
        st->selected = (!masterSelected && selectedChannel == static_cast<uint32_t>(i));
        if (auto* colArr = ch["color"].getArray(); colArr && colArr->size() >= 3)
            st->chColour = juce::Colour::fromRGB(
                static_cast<uint8_t>(static_cast<int>((*colArr)[0])),
                static_cast<uint8_t>(static_cast<int>((*colArr)[1])),
                static_cast<uint8_t>(static_cast<int>((*colArr)[2])));

        auto dsp = ch["dsp"];
        st->fader.setValue(static_cast<double>(dsp["fader_db"]), juce::dontSendNotification);
        st->muteBtn.setToggleState(static_cast<bool>(dsp["mute"]), juce::dontSendNotification);
        st->soloBtn.setToggleState(static_cast<bool>(dsp["solo"]), juce::dontSendNotification);
        st->recBtn.setToggleState(static_cast<bool>(dsp["record_arm"]), juce::dontSendNotification);

        st->onSelect = [this](uint32_t idx)
        {
            masterSelected = false;
            selectedChannel = idx;
            populateSelectedChannelControls();
            rebuildChannelStrips();
            repaint();
        };
        st->onFader = [this](uint32_t idx, float db)
        {
            auto d = ChronomixBridge::takeJson(chronomix_get_channel_dsp_json(bridge.raw(), idx));
            if (auto* o = d.getDynamicObject())
            {
                o->setProperty("fader_db", db);
                chronomix_set_channel_dsp_json(bridge.raw(), idx, juce::JSON::toString(d, true).toRawUTF8());
                if (idx == selectedChannel && !masterSelected)
                    repaint(lcdFooterBounds);
            }
        };
        st->onToggleMute = [this](uint32_t idx)
        {
            auto d = ChronomixBridge::takeJson(chronomix_get_channel_dsp_json(bridge.raw(), idx));
            if (auto* o = d.getDynamicObject())
            {
                o->setProperty("mute", !static_cast<bool>(d["mute"]));
                chronomix_set_channel_dsp_json(bridge.raw(), idx, juce::JSON::toString(d, true).toRawUTF8());
                refreshState();
            }
        };
        st->onToggleSolo = [this](uint32_t idx)
        {
            auto d = ChronomixBridge::takeJson(chronomix_get_channel_dsp_json(bridge.raw(), idx));
            if (auto* o = d.getDynamicObject())
            {
                o->setProperty("solo", !static_cast<bool>(d["solo"]));
                chronomix_set_channel_dsp_json(bridge.raw(), idx, juce::JSON::toString(d, true).toRawUTF8());
                refreshState();
            }
        };
        st->onToggleRec = [this](uint32_t idx)
        {
            auto d = ChronomixBridge::takeJson(chronomix_get_channel_dsp_json(bridge.raw(), idx));
            if (auto* o = d.getDynamicObject())
            {
                o->setProperty("record_arm", !static_cast<bool>(d["record_arm"]));
                chronomix_set_channel_dsp_json(bridge.raw(), idx, juce::JSON::toString(d, true).toRawUTF8());
                refreshState();
            }
        };
        st->onOpenFx = [this](uint32_t idx)
        {
            masterSelected = false;
            selectedChannel = idx;
            setActiveTab(MixerTab::Vst);
            populateSelectedChannelControls();
            rebuildChannelStrips();
        };

        st->setBounds(x, 2, stripW, stripH);
        stripsContent.addAndMakeVisible(st.get());
        channelStrips.push_back(std::move(st));
        x += stripW + 5;
    }

    stripsContent.setSize(juce::jmax(stripsViewport.getWidth(), x + 8), stripH + 4);
}

void MixerPanel::updateTelemetry(const CMixerTelemetry& tel)
{
    lastTel = tel;
    monStartBtn.setToggleState(tel.is_audio_running != 0, juce::dontSendNotification);
    monStartBtn.setButtonText(tel.is_audio_running != 0 ? "AUDIO LIVE" : "AUDIO OFF");
    ioStartStopBtn.setToggleState(tel.is_audio_running != 0, juce::dontSendNotification);
    ioStartStopBtn.setButtonText(tel.is_audio_running != 0 ? "DETENER AUDIO" : "INICIAR AUDIO");

    monRecBtn.setToggleState(tel.is_recording != 0, juce::dontSendNotification);
    ioRecToggleBtn.setToggleState(tel.is_recording != 0, juce::dontSendNotification);
    ioRecToggleBtn.setButtonText(tel.is_recording != 0 ? "DETENER GRABACION (REC)" : "GRABAR MULTIPISTA (REC)");
    monVmicBtn.setToggleState(tel.is_virtual_mic_active != 0, juce::dontSendNotification);

    if (!masterSelected && selectedChannel < tel.channel_count && selectedChannel < 48)
    {
        const auto& cm = tel.channels[selectedChannel];
        homeMiniMeter.setLevels(cm.in_left, cm.in_right, cm.out_left, cm.out_right, cm.comp_gr_db);
    }
    else
    {
        homeMiniMeter.setLevels(tel.master_left, tel.master_right, tel.master_left, tel.master_right, 0.0f);
    }

    for (auto& st : channelStrips)
    {
        if (st->chIdx < tel.channel_count && st->chIdx < 48)
        {
            st->meterL = tel.channels[st->chIdx].out_left;
            st->meterR = tel.channels[st->chIdx].out_right;
            st->repaint();
        }
    }
    repaint();
}

void MixerPanel::layoutActiveTabContent(int w)
{
    tabContent.clearLayout();
    int y = 4;

    if (activeTab == MixerTab::Home)
    {
        bool wide = w >= 560;
        if (wide)
        {
            int leftW = juce::jlimit(220, 290, w * 40 / 100);
            int rightW = w - leftW - 14;
            int topBlockH = 184;

            juce::Rectangle<int> preBox(4, y, leftW, topBlockH);
            tabContent.addSection(preBox, "PREAMP / PANORAMA", StudioColors::orange);
            auto pi = preBox.reduced(6).withTrimmedTop(20);
            int kw = (pi.getWidth() - 6) / 2;
            int kh = 64;
            tabContent.addFieldLabel({ pi.getX(), pi.getY(), kw, 11 }, "TRIM", StudioColors::textMuted, juce::Justification::centred);
            homeTrimSlider.setBounds(pi.getX(), pi.getY() + 10, kw, kh);
            tabContent.addFieldLabel({ pi.getX() + kw + 6, pi.getY(), kw, 11 }, "PAN", StudioColors::textMuted, juce::Justification::centred);
            homePanSlider.setBounds(pi.getX() + kw + 6, pi.getY() + 10, kw, kh);

            int row2Y = pi.getY() + 76;
            tabContent.addFieldLabel({ pi.getX(), row2Y, kw, 11 }, "BALANCE", StudioColors::textMuted, juce::Justification::centred);
            homeBalanceSlider.setBounds(pi.getX(), row2Y + 10, kw, kh);
            tabContent.addFieldLabel({ pi.getX() + kw + 6, row2Y, kw, 11 }, "STEREO WIDTH", StudioColors::textMuted, juce::Justification::centred);
            homeWidthSlider.setBounds(pi.getX() + kw + 6, row2Y + 10, kw, kh);

            juce::Rectangle<int> eqBox(leftW + 10, y, rightW, topBlockH);
            tabContent.addSection(eqBox, "CURVA EQ / DINAMICA & MEDIDORES", StudioColors::cyan);
            auto ei = eqBox.reduced(6).withTrimmedTop(20);
            auto swRow = ei.removeFromTop(20);
            int swW = (swRow.getWidth() - 15) / 6;
            homePhaseToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeHpfToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeLpfToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeEqToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeGateToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeCompToggle.setBounds(swRow);
            ei.removeFromTop(4);
            auto meterRow = ei.removeFromBottom(42);
            ei.removeFromBottom(4);
            homeEqCurve.setBounds(ei);
            homeMiniMeter.setBounds(meterRow);

            y += topBlockH + 6;
        }
        else
        {
            juce::Rectangle<int> preBox(4, y, w - 8, 108);
            tabContent.addSection(preBox, "PREAMP / PANORAMA", StudioColors::orange);
            auto pi = preBox.reduced(6).withTrimmedTop(20);
            int kw = (pi.getWidth() - 9) / 4;
            tabContent.addFieldLabel({ pi.getX(), pi.getY(), kw, 11 }, "TRIM", StudioColors::textMuted, juce::Justification::centred);
            homeTrimSlider.setBounds(pi.getX(), pi.getY() + 10, kw, 66);
            tabContent.addFieldLabel({ pi.getX() + (kw + 3), pi.getY(), kw, 11 }, "PAN", StudioColors::textMuted, juce::Justification::centred);
            homePanSlider.setBounds(pi.getX() + (kw + 3), pi.getY() + 10, kw, 66);
            tabContent.addFieldLabel({ pi.getX() + (kw + 3) * 2, pi.getY(), kw, 11 }, "BAL", StudioColors::textMuted, juce::Justification::centred);
            homeBalanceSlider.setBounds(pi.getX() + (kw + 3) * 2, pi.getY() + 10, kw, 66);
            tabContent.addFieldLabel({ pi.getX() + (kw + 3) * 3, pi.getY(), kw, 11 }, "WIDTH", StudioColors::textMuted, juce::Justification::centred);
            homeWidthSlider.setBounds(pi.getX() + (kw + 3) * 3, pi.getY() + 10, kw, 66);
            y += 114;

            juce::Rectangle<int> eqBox(4, y, w - 8, 162);
            tabContent.addSection(eqBox, "CURVA EQ / DINAMICA & MEDIDORES", StudioColors::cyan);
            auto ei = eqBox.reduced(6).withTrimmedTop(20);
            auto swRow = ei.removeFromTop(20);
            int swW = (swRow.getWidth() - 15) / 6;
            homePhaseToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeHpfToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeLpfToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeEqToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeGateToggle.setBounds(swRow.removeFromLeft(swW)); swRow.removeFromLeft(3);
            homeCompToggle.setBounds(swRow);
            ei.removeFromTop(4);
            auto meterRow = ei.removeFromBottom(40);
            ei.removeFromBottom(4);
            homeEqCurve.setBounds(ei);
            homeMiniMeter.setBounds(meterRow);
            y += 168;
        }

        juce::Rectangle<int> profBox(4, y, w - 8, 56);
        tabContent.addSection(profBox, "PERFILES DSP DEL CANAL", StudioColors::orange);
        auto pr = profBox.reduced(6).withTrimmedTop(22);
        int btnW = (pr.getWidth() - 8) / 3;
        homeBrowseProfilesBtn.setBounds(pr.removeFromLeft(btnW)); pr.removeFromLeft(4);
        homeLoadProfileBtn.setBounds(pr.removeFromLeft(btnW)); pr.removeFromLeft(4);
        homeSaveProfileBtn.setBounds(pr);
        y += 62;
    }
    else if (activeTab == MixerTab::Source)
    {
        juce::Rectangle<int> inBox(4, y, w - 8, 108);
        tabContent.addSection(inBox, "ASIGNACION DE ENTRADA DE AUDIO (CH IN)", StudioColors::cyan);
        auto ii = inBox.reduced(8).withTrimmedTop(22);
        int modeW = juce::jmin(180, ii.getWidth() / 3);
        int refW = 86;
        int devW = juce::jmax(120, ii.getWidth() - modeW - refW - 12);

        tabContent.addFieldLabel({ ii.getX(), ii.getY(), devW, 13 }, "DISPOSITIVO DE ENTRADA", StudioColors::textMuted);
        tabContent.addFieldLabel({ ii.getX() + devW + 6, ii.getY(), modeW, 13 }, "MODO / PAR DE CANALES", StudioColors::textMuted);
        srcDeviceBox.setBounds(ii.getX(), ii.getY() + 15, devW, 26);
        srcModeBox.setBounds(ii.getX() + devW + 6, ii.getY() + 15, modeW, 26);
        srcRefreshDevBtn.setBounds(ii.getRight() - refW, ii.getY() + 15, refW, 26);
        srcInputStatusLabel.setBounds(ii.getX(), ii.getY() + 48, ii.getWidth(), 22);
        y += 114;

        juce::Rectangle<int> idBox(4, y, w - 8, 96);
        tabContent.addSection(idBox, "IDENTIFICACION & COLOR DE TIRA", StudioColors::orange);
        auto idI = idBox.reduced(8).withTrimmedTop(22);
        tabContent.addFieldLabel({ idI.getX(), idI.getY(), 160, 13 }, "NOMBRE DEL CANAL", StudioColors::textMuted);
        srcNameEditor.setBounds(idI.getX(), idI.getY() + 14, juce::jmin(260, idI.getWidth()), 24);
        int sw = juce::jlimit(16, 26, (idI.getWidth() - 15 * 3) / 16);
        for (int i = 0; i < 16; ++i)
            srcColorBtns[static_cast<size_t>(i)].setBounds(idI.getX() + i * (sw + 3), idI.getY() + 44, sw, 20);
        y += 102;

        juce::Rectangle<int> medBox(4, y, w - 8, 86);
        tabContent.addSection(medBox, "REPRODUCTOR MULTIMEDIA DE CANAL", StudioColors::orange);
        auto mi = medBox.reduced(8).withTrimmedTop(24);
        int loadW = juce::jmin(220, mi.getWidth() / 2);
        srcLoadMediaBtn.setBounds(mi.getX(), mi.getY(), loadW, 26);
        srcMediaPlayBtn.setBounds(mi.getX() + loadW + 4, mi.getY(), 86, 26);
        srcMediaStopBtn.setBounds(mi.getX() + loadW + 94, mi.getY(), 58, 26);
        srcMediaLoopToggle.setBounds(mi.getX() + loadW + 156, mi.getY(), 66, 26);
        srcMediaInfoLabel.setBounds(mi.getX(), mi.getY() + 31, mi.getWidth(), 20);
        y += 92;
    }
    else if (activeTab == MixerTab::Eq)
    {
        juce::Rectangle<int> topBox(4, y, w - 8, 186);
        tabContent.addSection(topBox, "FILTROS DE CORTE & CURVA PARAMETRICA 4 BANDAS", StudioColors::orange);
        auto ti = topBox.reduced(6).withTrimmedTop(22);
        auto r1 = ti.removeFromTop(24);
        eqOnToggle.setBounds(r1.removeFromLeft(68)); r1.removeFromLeft(4);
        eqHpfToggle.setBounds(r1.removeFromLeft(88)); r1.removeFromLeft(2);
        eqHpfSlider.setBounds(r1.removeFromLeft(juce::jmin(126, r1.getWidth() / 3))); r1.removeFromLeft(6);
        eqLpfToggle.setBounds(r1.removeFromLeft(88)); r1.removeFromLeft(2);
        eqLpfSlider.setBounds(r1.removeFromLeft(juce::jmin(126, r1.getWidth())));
        ti.removeFromTop(4);
        auto r2 = ti.removeFromTop(22);
        eqOpenProfilesBtn.setBounds(r2.removeFromLeft(130)); r2.removeFromLeft(4);
        eqResetFlatBtn.setBounds(r2.removeFromLeft(90));
        ti.removeFromTop(4);
        eqMainCurve.setBounds(ti);
        y += 192;

        int cols = (w >= 540) ? 4 : 2;
        int cardW = (w - (cols + 1) * 6) / cols;
        int cardH = 136;
        const char* bandHdrs[4] = { "BANDA 1 - LOW SHELF", "BANDA 2 - LO-MID", "BANDA 3 - HI-MID", "BANDA 4 - HIGH SHELF" };
        for (int i = 0; i < 4; ++i)
        {
            int col = i % cols;
            int row = i / cols;
            int cx = 4 + col * (cardW + 6);
            int cy = y + row * (cardH + 6);
            juce::Rectangle<int> bBox(cx, cy, cardW, cardH);
            tabContent.addSection(bBox, bandHdrs[i], StudioColors::orange);
            auto bi = bBox.reduced(6).withTrimmedTop(21);
            eqBands[static_cast<size_t>(i)].enabledToggle.setBounds(bi.removeFromTop(19));
            bi.removeFromTop(2);
            eqBands[static_cast<size_t>(i)].freqSlider.setBounds(bi.removeFromTop(21));
            bi.removeFromTop(2);
            eqBands[static_cast<size_t>(i)].gainSlider.setBounds(bi.removeFromTop(21));
            bi.removeFromTop(2);
            eqBands[static_cast<size_t>(i)].satMeter.setBounds(bi.removeFromTop(21));
            bi.removeFromTop(2);
            eqBands[static_cast<size_t>(i)].qSlider.setBounds(bi.removeFromTop(21));
        }
        y += ((4 + cols - 1) / cols) * (cardH + 6) + 4;
    }
    else if (activeTab == MixerTab::Dyn)
    {
        juce::Rectangle<int> dynBox(4, y, w - 8, 152);
        tabContent.addSection(dynBox, "GATE / COMPRESOR / LIMITADOR", StudioColors::orange);
        auto di = dynBox.reduced(6).withTrimmedTop(22);
        dynCurve.setBounds(di.removeFromLeft(142));
        di.removeFromLeft(8);
        int half = (di.getWidth() - 6) / 2;
        auto leftHalf = di.removeFromLeft(half);
        di.removeFromLeft(6);
        auto rightHalf = di;

        dynGateToggle.setBounds(leftHalf.removeFromTop(20));
        dynGateThr.setBounds(leftHalf.removeFromTop(22));
        leftHalf.removeFromTop(8);
        dynLimToggle.setBounds(leftHalf.removeFromTop(20));
        dynLimCeil.setBounds(leftHalf.removeFromTop(22));

        dynCompToggle.setBounds(rightHalf.removeFromTop(20));
        dynCompThr.setBounds(rightHalf.removeFromTop(22));
        dynCompRatio.setBounds(rightHalf.removeFromTop(22));
        dynCompMakeup.setBounds(rightHalf.removeFromTop(22));
        y += 158;

        int colW = (w - 16) / 3;
        int fxH = 148;
        juce::Rectangle<int> fx1(4, y, colW, fxH);
        tabContent.addSection(fx1, "DE-ESSER & DRIVE", StudioColors::cyan);
        auto f1 = fx1.reduced(6).withTrimmedTop(22);
        dynDeessToggle.setBounds(f1.removeFromTop(20));
        dynDeessFreq.setBounds(f1.removeFromTop(21));
        dynDeessThr.setBounds(f1.removeFromTop(21));
        f1.removeFromTop(6);
        dynDriveToggle.setBounds(f1.removeFromTop(20));
        dynDriveAmt.setBounds(f1.removeFromTop(21));

        juce::Rectangle<int> fx2(8 + colW, y, colW, fxH);
        tabContent.addSection(fx2, "STEREO DELAY", StudioColors::cyan);
        auto f2 = fx2.reduced(6).withTrimmedTop(22);
        dynDelayToggle.setBounds(f2.removeFromTop(20));
        dynDelayTime.setBounds(f2.removeFromTop(22));
        dynDelayFb.setBounds(f2.removeFromTop(22));
        dynDelayMix.setBounds(f2.removeFromTop(22));

        juce::Rectangle<int> fx3(12 + colW * 2, y, colW, fxH);
        tabContent.addSection(fx3, "STUDIO REVERB", StudioColors::cyan);
        auto f3 = fx3.reduced(6).withTrimmedTop(22);
        dynRevToggle.setBounds(f3.removeFromTop(20));
        dynRevRoom.setBounds(f3.removeFromTop(22));
        dynRevDamp.setBounds(f3.removeFromTop(22));
        dynRevMix.setBounds(f3.removeFromTop(22));
        y += fxH + 6;
    }
    else if (activeTab == MixerTab::Profiles)
    {
        juce::Rectangle<int> barBox(4, y, w - 8, 56);
        tabContent.addSection(barBox, "GESTION DE PERFILES DSP (.JSON)", StudioColors::orange);
        auto bi = barBox.reduced(6).withTrimmedTop(22);
        int bw = (bi.getWidth() - 8) / 3;
        profLoadJsonBtn.setBounds(bi.removeFromLeft(bw)); bi.removeFromLeft(4);
        profSaveJsonBtn.setBounds(bi.removeFromLeft(bw)); bi.removeFromLeft(4);
        profResetFlatBtn.setBounds(bi);
        y += 62;

        for (auto& row : profileRows)
        {
            row->setBounds(4, y, w - 8, 62);
            y += 66;
        }
    }
    else if (activeTab == MixerTab::Vst)
    {
        juce::Rectangle<int> rackBox(4, y, w - 8, vstWineWarningLabel.getText().isNotEmpty() ? 192 : 148);
        tabContent.addSection(rackBox, "RACK DE INSERTOS VST2 / VST3 (4 SLOTS)", StudioColors::purple);
        auto ri = rackBox.reduced(6).withTrimmedTop(22);
        auto slotRow = ri.removeFromTop(24);
        int sw = (slotRow.getWidth() - 12) / 4;
        for (int i = 0; i < 4; ++i)
        {
            vstSlotBtns[static_cast<size_t>(i)].setBounds(slotRow.removeFromLeft(sw));
            slotRow.removeFromLeft(4);
        }
        ri.removeFromTop(5);
        if (vstWineWarningLabel.getText().isNotEmpty())
        {
            vstWineWarningLabel.setBounds(ri.removeFromTop(36));
            ri.removeFromTop(5);
        }
        auto actRow = ri.removeFromTop(25);
        vstScanBtn.setBounds(actRow.removeFromLeft(96)); actRow.removeFromLeft(4);
        vstBrowseBtn.setBounds(actRow.removeFromLeft(174)); actRow.removeFromLeft(4);
        vstOpenGuiBtn.setBounds(actRow.removeFromLeft(116)); actRow.removeFromLeft(4);
        vstUnloadBtn.setBounds(actRow.removeFromLeft(84));
        ri.removeFromTop(5);
        auto libRow = ri.removeFromTop(25);
        vstLoadSelectedBtn.setBounds(libRow.removeFromRight(140));
        libRow.removeFromRight(4);
        vstDiscoveredBox.setBounds(libRow);
        ri.removeFromTop(5);
        auto mixRow = ri.removeFromTop(24);
        vstBypassToggle.setBounds(mixRow.removeFromLeft(82)); mixRow.removeFromLeft(4);
        vstMixSlider.setBounds(mixRow.removeFromLeft(175)); mixRow.removeFromLeft(6);
        vstParamFilter.setBounds(mixRow);
        y += rackBox.getHeight() + 6;

        juce::Rectangle<int> pBox(4, y, w - 8, 168);
        tabContent.addSection(pBox, "PARAMETROS DEL PLUGIN", StudioColors::cyan);
        vstParamsViewport.setBounds(pBox.reduced(6).withTrimmedTop(22));
        rebuildVstParams();
        y += pBox.getHeight() + 6;
    }
    else if (activeTab == MixerTab::IoRec)
    {
        juce::Rectangle<int> outBox(4, y, w - 8, 108);
        tabContent.addSection(outBox, "SALIDA PRINCIPAL (MAIN OUT & RELOJ DE AUDIO)", StudioColors::orange);
        auto oi = outBox.reduced(8).withTrimmedTop(22);
        int refW = 96;
        int outDevW = juce::jmax(150, oi.getWidth() - refW - 6);
        tabContent.addFieldLabel({ oi.getX(), oi.getY(), outDevW, 13 }, "DISPOSITIVO DE SALIDA DE AUDIO", StudioColors::textMuted);
        ioOutDeviceBox.setBounds(oi.getX(), oi.getY() + 14, outDevW, 25);
        ioRefreshBtn.setBounds(oi.getRight() - refW, oi.getY() + 14, refW, 25);

        int r2y = oi.getY() + 44;
        tabContent.addFieldLabel({ oi.getX(), r2y, 120, 12 }, "FRECUENCIA", StudioColors::textMuted);
        tabContent.addFieldLabel({ oi.getX() + 126, r2y, 130, 12 }, "BUFFER", StudioColors::textMuted);
        ioSampleRateBox.setBounds(oi.getX(), r2y + 13, 120, 25);
        ioBlockSizeBox.setBounds(oi.getX() + 126, r2y + 13, 130, 25);
        ioStartStopBtn.setBounds(oi.getX() + 264, r2y + 13, juce::jmax(120, oi.getWidth() - 264), 25);
        y += 114;

        juce::String inTitle = masterSelected
            ? "ENTRADA DE CANAL (SELECCIONA UN CANAL CH 01..48)"
            : ("ENTRADA DE AUDIO PARA CANAL #" + juce::String(selectedChannel + 1));
        juce::Rectangle<int> inBox(4, y, w - 8, 96);
        tabContent.addSection(inBox, inTitle, StudioColors::cyan);
        auto ii = inBox.reduced(8).withTrimmedTop(22);
        int modeW = juce::jmin(185, ii.getWidth() * 38 / 100);
        int devW = juce::jmax(130, ii.getWidth() - modeW - 6);
        tabContent.addFieldLabel({ ii.getX(), ii.getY(), devW, 13 }, "DISPOSITIVO DE ENTRADA (MICROFONO / INTERFAZ)", StudioColors::textMuted);
        tabContent.addFieldLabel({ ii.getX() + devW + 6, ii.getY(), modeW, 13 }, "MODO / CANALES", StudioColors::textMuted);
        ioInDeviceBox.setBounds(ii.getX(), ii.getY() + 14, devW, 25);
        ioInModeBox.setBounds(ii.getX() + devW + 6, ii.getY() + 14, modeW, 25);
        ioRoutingStatusLabel.setBounds(ii.getX(), ii.getY() + 45, ii.getWidth(), 20);
        y += 102;

        juce::Rectangle<int> recBox(4, y, w - 8, 110);
        tabContent.addSection(recBox, "MICROFONO VIRTUAL & GRABACION MULTIPISTA", StudioColors::red);
        auto ri = recBox.reduced(8).withTrimmedTop(22);
        ioVmicToggle.setBounds(ri.getX(), ri.getY(), 200, 24);
        ioVmicGainSlider.setBounds(ri.getX() + 206, ri.getY(), juce::jmax(120, ri.getWidth() - 206), 24);
        ioRecToggleBtn.setBounds(ri.getX(), ri.getY() + 30, 200, 28);
        ioOpenRecDirBtn.setBounds(ri.getX() + 206, ri.getY() + 30, 185, 28);
        ioRecDirLabel.setBounds(ri.getX(), ri.getY() + 62, ri.getWidth(), 20);
        y += 116;
    }
    else if (activeTab == MixerTab::Settings)
    {
        juce::Rectangle<int> genBox(4, y, w - 8, 166);
        tabContent.addSection(genBox, "AJUSTES GENERALES & RUTAS XDG", StudioColors::orange);
        auto gi = genBox.reduced(8).withTrimmedTop(24);
        setLangBox.setBounds(gi.getX(), gi.getY(), 150, 24);
        setSysVstToggle.setBounds(gi.getX() + 158, gi.getY(), 220, 24);
        setOpenVstDirBtn.setBounds(gi.getX(), gi.getY() + 30, 150, 24);
        setChooseVstDirBtn.setBounds(gi.getX() + 158, gi.getY() + 30, 200, 24);
        setPathsLabel.setBounds(gi.getX(), gi.getY() + 60, gi.getWidth(), 74);
        y += 172;

        juce::Rectangle<int> wineBox(4, y, w - 8, 168);
        tabContent.addSection(wineBox, "ENTORNO WINE PARA PLUGINS WINDOWS (.DLL / .VST3)", StudioColors::cyan);
        auto wi = wineBox.reduced(8).withTrimmedTop(24);
        setWineStatusLabel.setBounds(wi.getX(), wi.getY(), wi.getWidth(), 72);
        int bw = (wi.getWidth() - 8) / 3;
        setWineRefreshBtn.setBounds(wi.getX(), wi.getY() + 76, bw, 25);
        setWineInitBtn.setBounds(wi.getX() + bw + 4, wi.getY() + 76, bw, 25);
        setWineRebuildBtn.setBounds(wi.getX() + (bw + 4) * 2, wi.getY() + 76, bw, 25);
        setWineCfgBtn.setBounds(wi.getX(), wi.getY() + 106, bw, 25);
        setWineExplorerBtn.setBounds(wi.getX() + bw + 4, wi.getY() + 106, bw, 25);
        setWineRunExeBtn.setBounds(wi.getX() + (bw + 4) * 2, wi.getY() + 106, bw, 25);
        y += 174;
    }
    else if (activeTab == MixerTab::News)
    {
        juce::Rectangle<int> infoBox(4, y, w - 8, 240);
        tabContent.addSection(infoBox, "CHRONOMIX - INFORMACION DEL SISTEMA", StudioColors::orange);
        newsBodyLabel.setBounds(infoBox.reduced(10).withTrimmedTop(24));
        y += 246;
    }

    tabContent.setSize(w, juce::jmax(tabViewport.getHeight(), y + 6));
    tabContent.repaint();
}

void MixerPanel::paint(juce::Graphics& g)
{
    g.fillAll(StudioColors::bgDeep);

    if (!upperChassisBounds.isEmpty())
    {
        auto chF = upperChassisBounds.toFloat();
        g.setColour(juce::Colour(0xff12151d));
        g.fillRect(chF);
        g.setColour(juce::Colour(0xff2c3345));
        g.drawRect(chF, 1.0f);
        g.setColour(juce::Colour(0xff384158));
        g.drawHorizontalLine(upperChassisBounds.getY() + 1, chF.getX() + 1.0f, chF.getRight() - 1.0f);
        g.setColour(juce::Colour(0xff07080c));
        g.drawHorizontalLine(upperChassisBounds.getBottom() - 1, chF.getX() + 1.0f, chF.getRight() - 1.0f);
    }

    if (!leftKeyWellBounds.isEmpty())
    {
        g.setColour(juce::Colour(0xff090b10));
        g.fillRect(leftKeyWellBounds);
        g.setColour(StudioColors::borderSubtle);
        g.drawRect(leftKeyWellBounds, 1);

        auto titleRect = leftKeyWellBounds.withHeight(16);
        g.setColour(StudioColors::textMuted);
        g.setFont(juce::FontOptions(8.5f, juce::Font::bold));
        g.drawText("CONSOLA", titleRect, juce::Justification::centred);
    }

    if (!lcdBezelBounds.isEmpty())
    {
        auto bz = lcdBezelBounds.toFloat();
        g.setColour(juce::Colour(0xff05070a));
        g.fillRect(bz);
        g.setColour(juce::Colour(0xff3d465c));
        g.drawRect(bz, 1.5f);
        g.setColour(juce::Colour(0xff1f2636));
        g.drawRect(bz.reduced(2.5f), 1.0f);
    }

    if (!lcdHeaderBounds.isEmpty())
    {
        g.setColour(juce::Colour(0xff0d121c));
        g.fillRect(lcdHeaderBounds);
        g.setColour(StudioColors::orange.withAlpha(0.75f));
        g.drawHorizontalLine(lcdHeaderBounds.getBottom() - 1,
                             static_cast<float>(lcdHeaderBounds.getX()),
                             static_cast<float>(lcdHeaderBounds.getRight()));

        juce::Colour chCol = StudioColors::orange;
        juce::String tagCode = "MAIN";
        juce::String tagName = "MASTER BUS";
        if (!masterSelected)
        {
            auto ch = getSelectedChannelVar();
            tagCode = "CH " + juce::String(selectedChannel + 1).paddedLeft('0', 2);
            tagName = ch["name"].toString();
            if (auto* colArr = ch["color"].getArray(); colArr && colArr->size() >= 3)
                chCol = juce::Colour::fromRGB(
                    static_cast<uint8_t>(static_cast<int>((*colArr)[0])),
                    static_cast<uint8_t>(static_cast<int>((*colArr)[1])),
                    static_cast<uint8_t>(static_cast<int>((*colArr)[2])));
        }

        auto tagBox = juce::Rectangle<int>(lcdHeaderBounds.getX() + 4, lcdHeaderBounds.getY() + 3, 148, lcdHeaderBounds.getHeight() - 6);
        g.setColour(juce::Colour(0xff161c28));
        g.fillRect(tagBox);
        g.setColour(chCol);
        g.fillRect(tagBox.getX(), tagBox.getY(), 5, tagBox.getHeight());
        g.setColour(StudioColors::borderBright);
        g.drawRect(tagBox, 1);

        g.setColour(StudioColors::orange);
        g.setFont(juce::FontOptions(10.0f, juce::Font::bold));
        g.drawText(tagCode, tagBox.getX() + 8, tagBox.getY(), 42, tagBox.getHeight(), juce::Justification::centredLeft);
        g.setColour(StudioColors::textPrimary);
        g.setFont(juce::FontOptions(10.5f, juce::Font::bold));
        g.drawFittedText(tagName, tagBox.getX() + 52, tagBox.getY(), tagBox.getWidth() - 56, tagBox.getHeight(), juce::Justification::centredLeft, 1);

        int ledAreaW = 148;
        if (lcdHeaderBounds.getWidth() >= 560)
        {
            auto ledRect = juce::Rectangle<int>(lcdHeaderBounds.getRight() - ledAreaW - 4, lcdHeaderBounds.getY() + 4, ledAreaW, lcdHeaderBounds.getHeight() - 8);
            bool isLive = (lastTel.is_audio_running != 0);
            bool isRec = (lastTel.is_recording != 0);
            uint32_t sr = (lastTel.sample_rate > 0) ? lastTel.sample_rate : 48000;
            juce::String clk = juce::String(static_cast<double>(sr) / 1000.0, 1) + "k";

            g.setColour(StudioColors::textSecond);
            g.setFont(juce::FontOptions(9.5f, juce::Font::bold));
            g.drawText(clk, ledRect.removeFromLeft(36), juce::Justification::centred);

            auto drawLedPill = [&](juce::Rectangle<int> pr, const juce::String& txt, bool on, juce::Colour onCol)
            {
                g.setColour(on ? onCol.withAlpha(0.24f) : juce::Colour(0xff121620));
                g.fillRect(pr);
                g.setColour(on ? onCol : StudioColors::borderSubtle);
                g.drawRect(pr, 1);
                g.setColour(on ? onCol : StudioColors::textMuted);
                g.setFont(juce::FontOptions(8.5f, juce::Font::bold));
                g.drawText(txt, pr, juce::Justification::centred);
            };

            drawLedPill(ledRect.removeFromLeft(38), isLive ? "LIVE" : "IDLE", isLive, StudioColors::green);
            ledRect.removeFromLeft(3);
            drawLedPill(ledRect.removeFromLeft(32), "REC", isRec, StudioColors::red);
            ledRect.removeFromLeft(3);
            drawLedPill(ledRect, "VMIC", lastTel.is_virtual_mic_active != 0, StudioColors::cyan);
        }
    }

    if (!lcdFooterBounds.isEmpty())
    {
        g.setColour(juce::Colour(0xff0c1018));
        g.fillRect(lcdFooterBounds);
        g.setColour(StudioColors::borderSubtle);
        g.drawHorizontalLine(lcdFooterBounds.getY(),
                             static_cast<float>(lcdFooterBounds.getX()),
                             static_cast<float>(lcdFooterBounds.getRight()));

        juce::String cells[6][2];
        juce::Colour cellCols[6] = {
            StudioColors::orange, StudioColors::cyan, StudioColors::green,
            StudioColors::yellow, StudioColors::cyan, StudioColors::orange
        };
        if (masterSelected)
        {
            cells[0][0] = "BUS"; cells[0][1] = "MASTER L/R";
            cells[1][0] = "MUTE"; cells[1][1] = masterMuteBtn.getToggleState() ? "ON" : "OFF";
            cells[2][0] = "DIM"; cells[2][1] = masterDimBtn.getToggleState() ? "-20 dB" : "OFF";
            cells[3][0] = "REC MIX"; cells[3][1] = masterRecMixBtn.getToggleState() ? "ARMED" : "OFF";
            cells[4][0] = "VMIC"; cells[4][1] = lastTel.is_virtual_mic_active != 0 ? "ACTIVO" : "OFF";
            cells[5][0] = "FADER"; cells[5][1] = juce::String(masterFader.getValue(), 1) + " dB";
        }
        else
        {
            double trimVal = homeTrimSlider.getValue();
            double panVal = homePanSlider.getValue();
            juce::String panTxt = (std::abs(panVal) < 0.03) ? "C"
                                : (panVal < 0.0 ? ("L" + juce::String(static_cast<int>(std::round(-panVal * 100.0))))
                                                : ("R" + juce::String(static_cast<int>(std::round(panVal * 100.0)))));
            cells[0][0] = "TRIM"; cells[0][1] = (trimVal >= 0.0 ? "+" : "") + juce::String(trimVal, 1) + " dB";
            cells[1][0] = "HPF"; cells[1][1] = eqHpfToggle.getToggleState() ? (juce::String(static_cast<int>(eqHpfSlider.getValue())) + " Hz") : "OFF";
            cells[2][0] = "EQ"; cells[2][1] = eqOnToggle.getToggleState() ? "ON" : "BYPASS";
            cells[3][0] = "DYN"; cells[3][1] = juce::String(dynGateToggle.getToggleState() ? "G " : "") + (dynCompToggle.getToggleState() ? "COMP" : (dynGateToggle.getToggleState() ? "GATE" : "OFF"));
            cells[4][0] = "PAN"; cells[4][1] = panTxt;
            auto chVar = getSelectedChannelVar();
            double fdb = static_cast<double>(chVar["dsp"]["fader_db"]);
            cells[5][0] = "FADER"; cells[5][1] = juce::String(fdb, 1) + " dB";
        }

        int cw = lcdFooterBounds.getWidth() / 6;
        for (int i = 0; i < 6; ++i)
        {
            auto cRect = juce::Rectangle<int>(lcdFooterBounds.getX() + i * cw, lcdFooterBounds.getY() + 1, cw, lcdFooterBounds.getHeight() - 1);
            if (i > 0)
            {
                g.setColour(StudioColors::borderSubtle);
                g.drawVerticalLine(cRect.getX(), static_cast<float>(cRect.getY() + 2), static_cast<float>(cRect.getBottom() - 2));
            }
            g.setColour(StudioColors::textMuted);
            g.setFont(juce::FontOptions(8.5f, juce::Font::bold));
            g.drawText(cells[i][0], cRect.getX() + 5, cRect.getY(), 38, cRect.getHeight(), juce::Justification::centredLeft);
            g.setColour(cellCols[i]);
            g.setFont(juce::FontOptions(9.5f, juce::Font::bold));
            g.drawFittedText(cells[i][1], cRect.getX() + 42, cRect.getY(), juce::jmax(10, cRect.getWidth() - 46), cRect.getHeight(), juce::Justification::centredRight, 1);
        }
    }

    if (!monitorFrameBounds.isEmpty())
    {
        auto mf = monitorFrameBounds.toFloat();
        g.setColour(juce::Colour(0xff0b0d13));
        g.fillRect(mf);
        g.setColour(StudioColors::borderBright);
        g.drawRect(mf, 1.0f);

        auto hdr = monitorFrameBounds.withHeight(18);
        g.setColour(juce::Colour(0xff171b26));
        g.fillRect(hdr);
        g.setColour(StudioColors::orange);
        g.fillRect(hdr.getX(), hdr.getY(), 3, hdr.getHeight());
        g.setFont(juce::FontOptions(9.0f, juce::Font::bold));
        g.drawText("MONITOR / BUS", hdr.withTrimmedLeft(8), juce::Justification::centredLeft);
        g.setColour(StudioColors::borderSubtle);
        g.drawHorizontalLine(hdr.getBottom(), mf.getX(), mf.getRight());
    }

    if (!monMasterMeterL.isEmpty())
    {
        ChronomixLookAndFeel::drawSegmentedMeter(g, monMasterMeterL, lastTel.master_left, true, 18);
        ChronomixLookAndFeel::drawSegmentedMeter(g, monMasterMeterR, lastTel.master_right, true, 18);
        ChronomixLookAndFeel::drawSegmentedMeter(g, monVmicMeter, juce::jmax(lastTel.virtual_mic_left, lastTel.virtual_mic_right), true, 18);

        g.setColour(StudioColors::textMuted);
        g.setFont(juce::FontOptions(8.0f, juce::Font::bold));
        g.drawText("L", static_cast<int>(monMasterMeterL.getX()), static_cast<int>(monMasterMeterL.getBottom()) + 1, static_cast<int>(monMasterMeterL.getWidth()), 11, juce::Justification::centred);
        g.drawText("R", static_cast<int>(monMasterMeterR.getX()), static_cast<int>(monMasterMeterR.getBottom()) + 1, static_cast<int>(monMasterMeterR.getWidth()), 11, juce::Justification::centred);
        g.drawText("VM", static_cast<int>(monVmicMeter.getX()) - 2, static_cast<int>(monVmicMeter.getBottom()) + 1, static_cast<int>(monVmicMeter.getWidth()) + 4, 11, juce::Justification::centred);
    }

    if (!masterStripBounds.isEmpty())
    {
        g.setColour(masterSelected ? StudioColors::bgStripSel : juce::Colour(0xff171513));
        g.fillRect(masterStripBounds);
        g.setColour(masterSelected ? StudioColors::orange : StudioColors::borderBright);
        g.drawRect(masterStripBounds, masterSelected ? 1.5f : 1.0f);

        auto hdr = masterStripBounds.reduced(4.0f).removeFromTop(24.0f);
        g.setColour(StudioColors::orange);
        g.fillRect(hdr);
        g.setColour(juce::Colours::black);
        g.setFont(juce::FontOptions(10.0f, juce::Font::bold));
        g.drawText("MASTER BUS", hdr.toNearestInt(), juce::Justification::centred);

        ChronomixLookAndFeel::drawSegmentedMeter(g, masterMeterLRect, lastTel.master_left, true, 14);
        ChronomixLookAndFeel::drawSegmentedMeter(g, masterMeterRRect, lastTel.master_right, true, 14);
    }
}

void MixerPanel::paintOverChildren(juce::Graphics& g)
{
    auto chVar = getSelectedChannelVar();
    bool hasIn = !masterSelected && static_cast<int>(chVar["source_mode"]) != 0;
    bool eqActive = !masterSelected && eqOnToggle.getToggleState();
    bool dynActive = !masterSelected && (dynGateToggle.getToggleState() || dynCompToggle.getToggleState()
        || dynDeessToggle.getToggleState() || dynDriveToggle.getToggleState()
        || dynDelayToggle.getToggleState() || dynRevToggle.getToggleState() || dynLimToggle.getToggleState());
    bool vstActive = false;
    auto insVar = masterSelected ? cachedState["master"]["inserts"] : chVar["inserts"];
    if (auto* slots = insVar.getArray())
        for (int s = 0; s < slots->size(); ++s)
            if (static_cast<bool>((*slots)[s]["loaded"]))
                vstActive = true;

    const bool ledFlags[9] = {
        false, hasIn, eqActive, dynActive, false, vstActive, lastTel.is_audio_running != 0, false, false
    };
    const juce::Colour ledCols[9] = {
        StudioColors::orange, StudioColors::cyan, StudioColors::green,
        StudioColors::orange, StudioColors::orange, StudioColors::purple,
        StudioColors::green, StudioColors::textMuted, StudioColors::textMuted
    };

    for (size_t i = 0; i < 9; ++i)
    {
        auto b = leftMenuBtns[i].getBounds().toFloat();
        if (b.isEmpty())
            continue;
        bool active = (static_cast<int>(activeTab) == static_cast<int>(i));
        g.setColour(active ? StudioColors::orange : juce::Colour(0xff2b3140));
        g.fillRect(b.getX() + 1.0f, b.getY() + 2.0f, 3.5f, b.getHeight() - 4.0f);
        if (ledFlags[i])
        {
            g.setColour(ledCols[i]);
            g.fillEllipse(b.getRight() - 9.0f, b.getCentreY() - 2.5f, 5.0f, 5.0f);
        }
    }
}

void MixerPanel::resized()
{
    auto area = getLocalBounds().reduced(6);
    int upperH = juce::jlimit(278, 455, area.getHeight() * 50 / 100);
    auto upperDeck = area.removeFromTop(upperH);
    area.removeFromTop(8);
    auto lowerBank = area;

    const int rightMargin = 16;
    upperDeck.removeFromRight(rightMargin);
    upperChassisBounds = upperDeck;
    auto chassisInner = upperDeck.reduced(6);

    int monitorW = (chassisInner.getWidth() >= 820) ? juce::jlimit(182, 220, chassisInner.getWidth() * 20 / 100) : 0;
    bool showMon = (monitorW > 0);
    monStartBtn.setVisible(showMon);
    monRecBtn.setVisible(showMon);
    monVmicBtn.setVisible(showMon);
    monClearSoloBtn.setVisible(showMon);
    monPlayAllBtn.setVisible(showMon);

    if (showMon)
    {
        monitorFrameBounds = chassisInner.removeFromRight(monitorW);
        chassisInner.removeFromRight(6);
    }
    else
    {
        monitorFrameBounds = {};
    }

    bool narrow = chassisInner.getWidth() < 560;
    if (narrow)
    {
        leftKeyWellBounds = chassisInner.removeFromTop(58);
        chassisInner.removeFromTop(5);
        auto kInner = leftKeyWellBounds.reduced(3).withTrimmedTop(14);
        int perRow = 5;
        int bw = (kInner.getWidth() - (perRow - 1) * 2) / perRow;
        for (int i = 0; i < 9; ++i)
        {
            int r = i / perRow;
            int c = i % perRow;
            leftMenuBtns[static_cast<size_t>(i)].setBounds(kInner.getX() + c * (bw + 2), kInner.getY() + r * 20, bw, 18);
        }
    }
    else
    {
        leftKeyWellBounds = chassisInner.removeFromLeft(94);
        chassisInner.removeFromLeft(6);
        auto kInner = leftKeyWellBounds.reduced(4).withTrimmedTop(16);
        int bh = juce::jlimit(20, 32, (kInner.getHeight() - 8 * 3) / 9);
        for (int i = 0; i < 9; ++i)
            leftMenuBtns[static_cast<size_t>(i)].setBounds(kInner.getX(), kInner.getY() + i * (bh + 3), kInner.getWidth(), bh);
    }

    lcdBezelBounds = chassisInner;
    auto lcdInner = lcdBezelBounds.reduced(4);

    lcdHeaderBounds = lcdInner.removeFromTop(28);
    int hdrRightReserve = (lcdHeaderBounds.getWidth() >= 560) ? 156 : 4;
    auto hdrWork = lcdHeaderBounds.withTrimmedLeft(156).withTrimmedRight(hdrRightReserve);
    prevChBtn.setBounds(hdrWork.removeFromLeft(22).reduced(0, 3));
    hdrWork.removeFromLeft(2);
    nextChBtn.setBounds(hdrWork.removeFromLeft(22).reduced(0, 3));
    hdrWork.removeFromLeft(6);

    int routeSpace = hdrWork.getWidth();
    if (routeSpace >= 160)
    {
        int halfR = (routeSpace - 4) / 2;
        lcdInQuickBtn.setVisible(true);
        lcdOutQuickBtn.setVisible(true);
        lcdInQuickBtn.setBounds(hdrWork.removeFromLeft(halfR).reduced(0, 3));
        hdrWork.removeFromLeft(4);
        lcdOutQuickBtn.setBounds(hdrWork.reduced(0, 3));
    }
    else
    {
        lcdInQuickBtn.setVisible(false);
        lcdOutQuickBtn.setVisible(false);
    }

    lcdFooterBounds = lcdInner.removeFromBottom(21);
    tabViewport.setBounds(lcdInner);
    layoutActiveTabContent(juce::jmax(290, lcdInner.getWidth() - 14));

    if (showMon)
    {
        auto mInner = monitorFrameBounds.reduced(6).withTrimmedTop(20);
        auto mBtns = mInner.removeFromLeft(mInner.getWidth() - 58);
        mInner.removeFromLeft(6);
        int mbh = juce::jlimit(24, 34, (mBtns.getHeight() - 16) / 5);
        monStartBtn.setBounds(mBtns.removeFromTop(mbh)); mBtns.removeFromTop(4);
        monRecBtn.setBounds(mBtns.removeFromTop(mbh)); mBtns.removeFromTop(4);
        monVmicBtn.setBounds(mBtns.removeFromTop(mbh)); mBtns.removeFromTop(4);
        monClearSoloBtn.setBounds(mBtns.removeFromTop(mbh)); mBtns.removeFromTop(4);
        monPlayAllBtn.setBounds(mBtns.removeFromTop(mbh));

        auto meterCol = mInner.withTrimmedBottom(12);
        monMasterMeterL = meterCol.removeFromLeft(14).toFloat();
        meterCol.removeFromLeft(3);
        monMasterMeterR = meterCol.removeFromLeft(14).toFloat();
        meterCol.removeFromLeft(6);
        monVmicMeter = meterCol.removeFromLeft(14).toFloat();
    }
    else
    {
        monMasterMeterL = {};
        monMasterMeterR = {};
        monVmicMeter = {};
    }

    const int masterRightMargin = 24;
    lowerBank.removeFromRight(masterRightMargin);

    auto bankCol = lowerBank.removeFromLeft(86);
    lowerBank.removeFromLeft(6);
    int bkh = 24;
    for (int i = 0; i < 5; ++i)
    {
        bankBtns[static_cast<size_t>(i)].setBounds(bankCol.removeFromTop(bkh));
        bankCol.removeFromTop(3);
    }
    bankCol.removeFromTop(4);
    addChBtn.setBounds(bankCol.removeFromTop(26));
    bankCol.removeFromTop(3);
    addMediaBtn.setBounds(bankCol.removeFromTop(26));

    int masterW = 104;
    auto masterArea = lowerBank.removeFromRight(masterW);
    lowerBank.removeFromRight(8);
    masterStripBounds = masterArea.toFloat();

    auto mWork = masterArea.reduced(4);
    mWork.removeFromTop(28);
    masterFxBtn.setBounds(mWork.removeFromTop(20));
    mWork.removeFromTop(2);
    auto mRow1 = mWork.removeFromTop(20);
    masterMuteBtn.setBounds(mRow1.removeFromLeft(mRow1.getWidth() / 2 - 1));
    mRow1.removeFromLeft(2);
    masterDimBtn.setBounds(mRow1);
    mWork.removeFromTop(2);
    masterRecMixBtn.setBounds(mWork.removeFromTop(20));
    mWork.removeFromTop(4);
    auto mMeters = mWork.removeFromRight(22);
    masterMeterLRect = mMeters.removeFromLeft(10).toFloat();
    mMeters.removeFromLeft(2);
    masterMeterRRect = mMeters.toFloat();
    masterFader.setBounds(mWork);

    stripsViewport.setBounds(lowerBank);
    rebuildChannelStrips();
}
