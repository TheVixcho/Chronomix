#include <juce_gui_basics/juce_gui_basics.h>
#include "FfiBridge.h"
#include "ChronomixLookAndFeel.h"
#include "MainComponent.h"

static juce::Rectangle<int> computeValidatedWindowBounds(const juce::File& stateFile)
{
    const auto& displays = juce::Desktop::getInstance().getDisplays();
    const auto* primary = displays.getPrimaryDisplay();
    juce::Rectangle<int> workArea = (primary != nullptr && !primary->userBounds.isEmpty())
        ? primary->userBounds.toNearestInt()
        : juce::Rectangle<int>(0, 0, 1280, 800);

    int defW = juce::jlimit(640, juce::jmax(640, static_cast<int>(workArea.getWidth() * 0.92)), 1260);
    int defH = juce::jlimit(460, juce::jmax(460, static_cast<int>(workArea.getHeight() * 0.90)), 800);
    juce::Rectangle<int> fallback(
        workArea.getX() + juce::jmax(0, (workArea.getWidth() - defW) / 2),
        workArea.getY() + juce::jmax(0, (workArea.getHeight() - defH) / 2),
        defW,
        defH);

    if (!stateFile.existsAsFile())
        return fallback;

    auto parsed = juce::JSON::parse(stateFile);
    if (!parsed.isObject())
        return fallback;

    int x = static_cast<int>(parsed["x"]);
    int y = static_cast<int>(parsed["y"]);
    int w = static_cast<int>(parsed["w"]);
    int h = static_cast<int>(parsed["h"]);
    if (w < 480 || h < 320)
        return fallback;

    juce::Rectangle<int> candidate(x, y, w, h);
    for (const auto& d : displays.displays)
    {
        auto area = d.userBounds.toNearestInt();
        if (area.isEmpty())
            continue;
        auto clampedW = juce::jmin(candidate.getWidth(), area.getWidth());
        auto clampedH = juce::jmin(candidate.getHeight(), area.getHeight());
        auto overlap = area.getIntersection(candidate);
        if (overlap.getWidth() >= 200 && overlap.getHeight() >= 160)
        {
            int cx = juce::jlimit(area.getX(), juce::jmax(area.getX(), area.getRight() - clampedW), candidate.getX());
            int cy = juce::jlimit(area.getY(), juce::jmax(area.getY(), area.getBottom() - clampedH), candidate.getY());
            return { cx, cy, clampedW, clampedH };
        }
    }

    return fallback;
}

class ChronomixMainWindow : public juce::DocumentWindow
{
public:
    ChronomixMainWindow(ChronomixBridge& bridgeRef, const juce::File& winStateFile)
        : DocumentWindow("Chronomix - Studio Audio DAW (JUCE + Rust)",
                         StudioColors::bgDeep,
                         DocumentWindow::allButtons),
          stateFile(winStateFile)
    {
        setUsingNativeTitleBar(true);
        setContentOwned(new MainComponent(bridgeRef), false);
        setResizable(true, true);
        setResizeLimits(480, 340, 7680, 4320);

        auto validBounds = computeValidatedWindowBounds(stateFile);
        setBounds(validBounds);
        setVisible(true);
    }

    void saveValidatedBounds() const
    {
        auto b = getBounds();
        const auto& displays = juce::Desktop::getInstance().getDisplays();
        for (const auto& d : displays.displays)
        {
            auto overlap = d.userBounds.toNearestInt().getIntersection(b);
            if (overlap.getWidth() >= 200 && overlap.getHeight() >= 160)
            {
                auto* obj = new juce::DynamicObject();
                obj->setProperty("x", b.getX());
                obj->setProperty("y", b.getY());
                obj->setProperty("w", b.getWidth());
                obj->setProperty("h", b.getHeight());
                stateFile.getParentDirectory().createDirectory();
                stateFile.replaceWithText(juce::JSON::toString(juce::var(obj), true));
                return;
            }
        }
    }

    void closeButtonPressed() override
    {
        saveValidatedBounds();
        juce::JUCEApplication::getInstance()->systemRequestedQuit();
    }

private:
    juce::File stateFile;
};

class ChronomixJuceApp : public juce::JUCEApplication
{
public:
    const juce::String getApplicationName() override { return "Chronomix"; }
    const juce::String getApplicationVersion() override { return "0.0.1"; }
    bool moreThanOneInstanceAllowed() override { return true; }

    void initialise(const juce::String&) override
    {
        lookAndFeel = std::make_unique<ChronomixLookAndFeel>();
        juce::LookAndFeel::setDefaultLookAndFeel(lookAndFeel.get());
        bridge = std::make_unique<ChronomixBridge>();
        auto paths = bridge->getPaths();
        juce::File winStateFile(paths["window_state_file"].toString());
        mainWindow = std::make_unique<ChronomixMainWindow>(*bridge, winStateFile);
    }

    void shutdown() override
    {
        if (mainWindow != nullptr)
            mainWindow->saveValidatedBounds();
        mainWindow.reset();
        bridge.reset();
        juce::LookAndFeel::setDefaultLookAndFeel(nullptr);
        lookAndFeel.reset();
    }

    void systemRequestedQuit() override
    {
        quit();
    }

private:
    std::unique_ptr<ChronomixLookAndFeel> lookAndFeel;
    std::unique_ptr<ChronomixBridge> bridge;
    std::unique_ptr<ChronomixMainWindow> mainWindow;
};

START_JUCE_APPLICATION(ChronomixJuceApp)
