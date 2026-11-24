#pragma once

#include <juce_gui_basics/juce_gui_basics.h>

namespace StudioColors
{
    inline const juce::Colour bgDeep       { 0xff0c0c0f };
    inline const juce::Colour bgChassis    { 0xff101116 };
    inline const juce::Colour bgBezel      { 0xff121318 };
    inline const juce::Colour bgScreen     { 0xff13151b };
    inline const juce::Colour bgCard       { 0xff181b23 };
    inline const juce::Colour bgCardHover  { 0xff222633 };
    inline const juce::Colour bgStrip      { 0xff14161c };
    inline const juce::Colour bgStripSel   { 0xff221e1b };
    inline const juce::Colour borderSubtle { 0xff2a2f3d };
    inline const juce::Colour borderBright { 0xff464f66 };
    inline const juce::Colour orange       { 0xfff58220 };
    inline const juce::Colour orangeDark   { 0xff9a4b0c };
    inline const juce::Colour cyan         { 0xff28c8f0 };
    inline const juce::Colour green        { 0xff24c369 };
    inline const juce::Colour yellow       { 0xfff5af23 };
    inline const juce::Colour red          { 0xffeb3038 };
    inline const juce::Colour purple       { 0xff9b4bf0 };
    inline const juce::Colour textPrimary  { 0xfff0f2f8 };
    inline const juce::Colour textSecond   { 0xffa6aec2 };
    inline const juce::Colour textMuted    { 0xff6e768a };
}

class ChronomixLookAndFeel : public juce::LookAndFeel_V4
{
public:
    ChronomixLookAndFeel()
    {
        setColour(juce::ResizableWindow::backgroundColourId, StudioColors::bgDeep);
        setColour(juce::Label::textColourId, StudioColors::textPrimary);
        setColour(juce::TextButton::buttonColourId, StudioColors::bgCard);
        setColour(juce::TextButton::buttonOnColourId, StudioColors::orange);
        setColour(juce::TextButton::textColourOffId, StudioColors::textPrimary);
        setColour(juce::TextButton::textColourOnId, juce::Colours::black);
        setColour(juce::ToggleButton::textColourId, StudioColors::textPrimary);
        setColour(juce::ToggleButton::tickColourId, StudioColors::orange);
        setColour(juce::ComboBox::backgroundColourId, StudioColors::bgCard);
        setColour(juce::ComboBox::outlineColourId, StudioColors::borderSubtle);
        setColour(juce::ComboBox::textColourId, StudioColors::textPrimary);
        setColour(juce::ComboBox::arrowColourId, StudioColors::orange);
        setColour(juce::PopupMenu::backgroundColourId, StudioColors::bgBezel);
        setColour(juce::PopupMenu::textColourId, StudioColors::textPrimary);
        setColour(juce::PopupMenu::highlightedBackgroundColourId, StudioColors::orange);
        setColour(juce::PopupMenu::highlightedTextColourId, juce::Colours::black);
        setColour(juce::TextEditor::backgroundColourId, StudioColors::bgDeep);
        setColour(juce::TextEditor::outlineColourId, StudioColors::borderSubtle);
        setColour(juce::TextEditor::focusedOutlineColourId, StudioColors::orange);
        setColour(juce::TextEditor::textColourId, StudioColors::textPrimary);
        setColour(juce::Slider::trackColourId, StudioColors::orange);
        setColour(juce::Slider::backgroundColourId, StudioColors::bgDeep);
        setColour(juce::Slider::thumbColourId, juce::Colour(0xffe6e8ef));
        setColour(juce::Slider::textBoxTextColourId, StudioColors::textPrimary);
        setColour(juce::Slider::textBoxBackgroundColourId, StudioColors::bgDeep);
        setColour(juce::Slider::textBoxOutlineColourId, StudioColors::borderSubtle);
        setColour(juce::ScrollBar::thumbColourId, StudioColors::borderBright);
        setColour(juce::ScrollBar::trackColourId, StudioColors::bgDeep);
    }

    void drawButtonBackground(juce::Graphics& g,
                              juce::Button& button,
                              const juce::Colour& backgroundColour,
                              bool isHighlighted,
                              bool isDown) override
    {
        auto bounds = button.getLocalBounds().toFloat();
        auto col = backgroundColour;
        if (isDown)
            col = col.brighter(0.15f);
        else if (isHighlighted)
            col = col.brighter(0.08f);

        g.setColour(col);
        g.fillRect(bounds);

        auto border = button.getToggleState()
            ? StudioColors::orange
            : (isHighlighted ? StudioColors::borderBright : StudioColors::borderSubtle);
        g.setColour(border);
        g.drawRect(bounds, 1.0f);
    }

    void drawToggleButton(juce::Graphics& g,
                          juce::ToggleButton& button,
                          bool isHighlighted,
                          bool isDown) override
    {
        juce::ignoreUnused(isDown);
        auto bounds = button.getLocalBounds().toFloat();
        auto boxSize = juce::jmin(14.0f, bounds.getHeight() - 4.0f);
        auto box = juce::Rectangle<float>(bounds.getX() + 2.0f,
                                          bounds.getCentreY() - boxSize * 0.5f,
                                          boxSize,
                                          boxSize);
        g.setColour(button.getToggleState() ? StudioColors::orange : StudioColors::bgDeep);
        g.fillRect(box);
        g.setColour(button.getToggleState() ? StudioColors::orange
                                            : (isHighlighted ? StudioColors::borderBright : StudioColors::borderSubtle));
        g.drawRect(box, 1.0f);

        if (button.getToggleState())
        {
            g.setColour(juce::Colours::black);
            g.fillRect(box.reduced(3.5f));
        }

        g.setColour(button.findColour(juce::ToggleButton::textColourId));
        g.setFont(juce::FontOptions(11.5f));
        g.drawFittedText(button.getButtonText(),
                         static_cast<int>(box.getRight() + 6.0f),
                         0,
                         juce::jmax(1, button.getWidth() - static_cast<int>(box.getRight() + 8.0f)),
                         button.getHeight(),
                         juce::Justification::centredLeft,
                         1);
    }

    void drawRotarySlider(juce::Graphics& g,
                          int x, int y, int width, int height,
                          float sliderPos,
                          float rotaryStartAngle,
                          float rotaryEndAngle,
                          juce::Slider& slider) override
    {
        auto bounds = juce::Rectangle<float>(static_cast<float>(x), static_cast<float>(y),
                                             static_cast<float>(width), static_cast<float>(height)).reduced(3.0f);
        auto radius = juce::jmin(bounds.getWidth(), bounds.getHeight()) * 0.5f;
        auto cx = bounds.getCentreX();
        auto cy = bounds.getCentreY();
        auto angle = rotaryStartAngle + sliderPos * (rotaryEndAngle - rotaryStartAngle);
        auto accent = slider.findColour(juce::Slider::rotarySliderFillColourId);
        if (accent.isTransparent())
            accent = StudioColors::orange;

        juce::Path bgArc;
        bgArc.addCentredArc(cx, cy, radius, radius, 0.0f, rotaryStartAngle, rotaryEndAngle, true);
        g.setColour(StudioColors::borderSubtle);
        g.strokePath(bgArc, juce::PathStrokeType(2.5f));

        juce::Path valArc;
        valArc.addCentredArc(cx, cy, radius, radius, 0.0f, rotaryStartAngle, angle, true);
        g.setColour(accent);
        g.strokePath(valArc, juce::PathStrokeType(2.8f));

        auto innerR = radius * 0.72f;
        g.setColour(juce::Colour(0xff1e212b));
        g.fillEllipse(cx - innerR, cy - innerR, innerR * 2.0f, innerR * 2.0f);
        g.setColour(StudioColors::borderBright);
        g.drawEllipse(cx - innerR, cy - innerR, innerR * 2.0f, innerR * 2.0f, 1.0f);

        juce::Path pointer;
        pointer.addRectangle(-1.2f, -innerR + 2.0f, 2.4f, innerR * 0.55f);
        g.setColour(accent);
        g.fillPath(pointer, juce::AffineTransform::rotation(angle).translated(cx, cy));
    }

    void drawLinearSlider(juce::Graphics& g,
                          int x, int y, int width, int height,
                          float sliderPos,
                          float minSliderPos,
                          float maxSliderPos,
                          juce::Slider::SliderStyle style,
                          juce::Slider& slider) override
    {
        juce::ignoreUnused(minSliderPos, maxSliderPos);
        auto area = juce::Rectangle<float>(static_cast<float>(x), static_cast<float>(y),
                                           static_cast<float>(width), static_cast<float>(height));
        auto accent = slider.findColour(juce::Slider::trackColourId);
        if (accent.isTransparent())
            accent = StudioColors::orange;

        if (style == juce::Slider::LinearVertical)
        {
            auto trackW = 5.0f;
            auto track = juce::Rectangle<float>(area.getCentreX() - trackW * 0.5f,
                                                area.getY() + 4.0f,
                                                trackW,
                                                area.getHeight() - 8.0f);
            g.setColour(juce::Colour(0xff090a0d));
            g.fillRect(track);
            g.setColour(StudioColors::borderSubtle);
            g.drawRect(track, 1.0f);

            auto fillTop = juce::jlimit(track.getY(), track.getBottom(), sliderPos);
            g.setColour(accent.withAlpha(0.75f));
            g.fillRect(juce::Rectangle<float>(track.getX() + 1.0f, fillTop,
                                              track.getWidth() - 2.0f, track.getBottom() - fillTop));

            auto capW = juce::jmin(26.0f, area.getWidth() - 2.0f);
            auto capH = 14.0f;
            auto cap = juce::Rectangle<float>(area.getCentreX() - capW * 0.5f,
                                              fillTop - capH * 0.5f,
                                              capW,
                                              capH);
            g.setColour(juce::Colour(0xffd8dce6));
            g.fillRect(cap);
            g.setColour(juce::Colour(0xff20232c));
            g.drawRect(cap, 1.0f);
            g.setColour(accent);
            g.fillRect(juce::Rectangle<float>(cap.getX() + 2.0f, cap.getCentreY() - 1.0f,
                                              cap.getWidth() - 4.0f, 2.0f));
        }
        else
        {
            auto trackH = 5.0f;
            auto track = juce::Rectangle<float>(area.getX() + 4.0f,
                                                area.getCentreY() - trackH * 0.5f,
                                                area.getWidth() - 8.0f,
                                                trackH);
            g.setColour(juce::Colour(0xff090a0d));
            g.fillRect(track);
            g.setColour(StudioColors::borderSubtle);
            g.drawRect(track, 1.0f);

            auto posX = juce::jlimit(track.getX(), track.getRight(), sliderPos);
            g.setColour(accent.withAlpha(0.8f));
            g.fillRect(juce::Rectangle<float>(track.getX() + 1.0f, track.getY() + 1.0f,
                                              juce::jmax(0.0f, posX - track.getX() - 1.0f),
                                              track.getHeight() - 2.0f));

            auto thumbW = 10.0f;
            auto thumbH = juce::jmin(18.0f, area.getHeight() - 2.0f);
            auto cap = juce::Rectangle<float>(posX - thumbW * 0.5f,
                                              area.getCentreY() - thumbH * 0.5f,
                                              thumbW,
                                              thumbH);
            g.setColour(juce::Colour(0xffe0e4ee));
            g.fillRect(cap);
            g.setColour(juce::Colour(0xff20232c));
            g.drawRect(cap, 1.0f);
            g.setColour(accent);
            g.fillRect(juce::Rectangle<float>(cap.getCentreX() - 1.0f, cap.getY() + 2.0f,
                                              2.0f, cap.getHeight() - 4.0f));
        }
    }

    void drawComboBox(juce::Graphics& g,
                      int width, int height,
                      bool isButtonDown,
                      int buttonX, int buttonY, int buttonW, int buttonH,
                      juce::ComboBox& box) override
    {
        juce::ignoreUnused(isButtonDown, buttonX, buttonY, buttonW, buttonH);
        auto bounds = juce::Rectangle<float>(0.0f, 0.0f, static_cast<float>(width), static_cast<float>(height));
        g.setColour(box.findColour(juce::ComboBox::backgroundColourId));
        g.fillRect(bounds);
        g.setColour(box.findColour(juce::ComboBox::outlineColourId));
        g.drawRect(bounds, 1.0f);

        juce::Path arrow;
        auto ax = static_cast<float>(width - 14);
        auto ay = static_cast<float>(height) * 0.5f;
        arrow.addTriangle(ax - 4.0f, ay - 2.5f, ax + 4.0f, ay - 2.5f, ax, ay + 3.0f);
        g.setColour(StudioColors::orange);
        g.fillPath(arrow);
    }

    static void drawSegmentedMeter(juce::Graphics& g,
                                   juce::Rectangle<float> bounds,
                                   float level,
                                   bool vertical,
                                   int segments = 14)
    {
        g.setColour(juce::Colour(0xff090a0d));
        g.fillRect(bounds);
        g.setColour(StudioColors::borderSubtle);
        g.drawRect(bounds, 1.0f);

        auto inner = bounds.reduced(1.5f);
        float norm = 0.0f;
        if (level > 1e-5f)
        {
            float db = 20.0f * std::log10(level);
            norm = juce::jlimit(0.0f, 1.0f, (db + 54.0f) / 60.0f);
        }
        int lit = static_cast<int>(std::round(norm * static_cast<float>(segments)));

        for (int i = 0; i < segments; ++i)
        {
            float frac = static_cast<float>(i) / static_cast<float>(juce::jmax(1, segments - 1));
            bool on = i < lit;
            juce::Colour col;
            if (frac > 0.82f)
                col = on ? StudioColors::red : juce::Colour(0xff381216);
            else if (frac > 0.62f)
                col = on ? StudioColors::yellow : juce::Colour(0xff3b2d0e);
            else
                col = on ? StudioColors::green : juce::Colour(0xff0f2e1c);

            g.setColour(col);
            if (vertical)
            {
                float segH = (inner.getHeight() - static_cast<float>(segments - 1)) / static_cast<float>(segments);
                float y = inner.getBottom() - static_cast<float>(i + 1) * segH - static_cast<float>(i);
                g.fillRect(juce::Rectangle<float>(inner.getX(), y, inner.getWidth(), juce::jmax(1.0f, segH)));
            }
            else
            {
                float segW = (inner.getWidth() - static_cast<float>(segments - 1)) / static_cast<float>(segments);
                float x = inner.getX() + static_cast<float>(i) * (segW + 1.0f);
                g.fillRect(juce::Rectangle<float>(x, inner.getY(), juce::jmax(1.0f, segW), inner.getHeight()));
            }
        }
    }

    static void drawEqSaturationBar(juce::Graphics& g,
                                    juce::Rectangle<float> bounds,
                                    float gainDb,
                                    bool enabled)
    {
        g.setColour(juce::Colour(0xff0b0d12));
        g.fillRect(bounds);
        g.setColour(StudioColors::borderSubtle);
        g.drawRect(bounds, 1.0f);

        constexpr int segments = 14;
        auto inner = bounds.reduced(1.0f);
        float segW = (inner.getWidth() - static_cast<float>(segments - 1)) / static_cast<float>(segments);
        float ratio = enabled ? juce::jlimit(0.0f, 1.0f, (gainDb + 18.0f) / 36.0f) : 0.0f;
        int activeSegs = static_cast<int>(std::round(ratio * static_cast<float>(segments)));

        for (int s = 0; s < segments; ++s)
        {
            float segGain = -18.0f + (static_cast<float>(s) / static_cast<float>(segments - 1)) * 36.0f;
            bool on = s < activeSegs;
            juce::Colour col;
            if (segGain > 7.0f)
                col = on ? StudioColors::red : juce::Colour(0xff381216);
            else if (segGain > 3.0f)
                col = on ? StudioColors::yellow : juce::Colour(0xff3b2d0e);
            else
                col = on ? StudioColors::green : juce::Colour(0xff0f2e1c);

            g.setColour(col);
            float sx = inner.getX() + static_cast<float>(s) * (segW + 1.0f);
            g.fillRect(juce::Rectangle<float>(sx, inner.getY(), juce::jmax(1.0f, segW), inner.getHeight()));
        }
    }
};
