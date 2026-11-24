#pragma once

#include <juce_gui_basics/juce_gui_basics.h>
#include "FfiBridge.h"
#include "ChronomixLookAndFeel.h"
#include <array>
#include <functional>
#include <memory>
#include <vector>

enum class MixerTab
{
    Home = 0,
    Source,
    Eq,
    Dyn,
    Profiles,
    Vst,
    IoRec,
    Settings,
    News
};

enum class BankFilter
{
    Ch1_16 = 0,
    Ch17_32,
    Ch33_48,
    Multimedia,
    All
};

struct LcdSectionBox
{
    juce::Rectangle<int> bounds;
    juce::String title;
    juce::Colour accent;
};

struct LcdFieldLabel
{
    juce::Rectangle<int> bounds;
    juce::String text;
    juce::Colour colour;
    juce::Justification align = juce::Justification::centredLeft;
};

class ConsoleTabCanvas : public juce::Component
{
public:
    void clearLayout()
    {
        sections.clear();
        labels.clear();
    }

    void addSection(juce::Rectangle<int> b, const juce::String& title, juce::Colour accent)
    {
        sections.push_back({ b, title, accent });
    }

    void addFieldLabel(juce::Rectangle<int> b, const juce::String& text,
                       juce::Colour col = StudioColors::textMuted,
                       juce::Justification align = juce::Justification::centredLeft)
    {
        labels.push_back({ b, text, col, align });
    }

    void paint(juce::Graphics& g) override;

private:
    std::vector<LcdSectionBox> sections;
    std::vector<LcdFieldLabel> labels;
};

class EqCurveDisplay : public juce::Component
{
public:
    EqCurveDisplay(ChronomixBridge& b);
    void setChannel(uint32_t chIdx, bool enabled, const std::array<float, 4>& freqs,
                    const std::array<float, 4>& gains, const std::array<bool, 4>& bandEn);
    void paint(juce::Graphics& g) override;

private:
    ChronomixBridge& bridge;
    uint32_t ch = 0;
    bool eqEnabled = true;
    std::array<float, 4> bandFreqs { 100.0f, 500.0f, 2500.0f, 8000.0f };
    std::array<float, 4> bandGains { 0.0f, 0.0f, 0.0f, 0.0f };
    std::array<bool, 4> bandEnabled { true, true, true, true };
};

class DynamicsCurveDisplay : public juce::Component
{
public:
    void setParams(bool gateOn, float gateThr, bool compOn, float compThr, float compRatio, float grDb);
    void paint(juce::Graphics& g) override;

private:
    bool gateEnabled = false;
    float gateThreshold = -45.0f;
    bool compEnabled = false;
    float compThreshold = -18.0f;
    float ratio = 3.0f;
    float gainReductionDb = 0.0f;
};

class EqBandSatMeter : public juce::Component
{
public:
    void setState(float gDb, bool en)
    {
        gainDb = gDb;
        enabled = en;
        repaint();
    }
    void paint(juce::Graphics& g) override;

private:
    float gainDb = 0.0f;
    bool enabled = true;
};

class ChannelMiniMeter : public juce::Component
{
public:
    void setLevels(float inL, float inR, float outL, float outR, float grDb)
    {
        inLeft = inL;
        inRight = inR;
        outLeft = outL;
        outRight = outR;
        compGrDb = grDb;
        repaint();
    }
    void paint(juce::Graphics& g) override;

private:
    float inLeft = 0.0f;
    float inRight = 0.0f;
    float outLeft = 0.0f;
    float outRight = 0.0f;
    float compGrDb = 0.0f;
};

class MixerPanel : public juce::Component
{
public:
    MixerPanel(ChronomixBridge& bridgeRef);
    void refreshState();
    void updateTelemetry(const CMixerTelemetry& tel);
    void assignMediaFileToNewOrSelectedChannel(const juce::File& file);
    std::function<void(const juce::String&)> onNotify;
    std::function<void()> onRefreshLibrary;

    void paint(juce::Graphics& g) override;
    void paintOverChildren(juce::Graphics& g) override;
    void resized() override;

private:
    ChronomixBridge& bridge;
    MixerTab activeTab = MixerTab::Home;
    BankFilter activeBank = BankFilter::Ch1_16;
    bool masterSelected = false;
    uint32_t selectedChannel = 0;
    uint32_t focusedVstSlot = 0;
    juce::var cachedState;
    juce::var cachedDevices;
    juce::var cachedPresets;
    juce::var cachedPlugins;
    juce::var cachedWineStatus;
    CMixerTelemetry lastTel {};
    bool updatingControls = false;

    juce::Rectangle<int> upperChassisBounds;
    juce::Rectangle<int> leftKeyWellBounds;
    juce::Rectangle<int> lcdBezelBounds;
    juce::Rectangle<int> lcdHeaderBounds;
    juce::Rectangle<int> lcdFooterBounds;
    juce::Rectangle<int> monitorFrameBounds;

    std::array<juce::TextButton, 9> leftMenuBtns;
    juce::TextButton prevChBtn { "<" };
    juce::TextButton nextChBtn { ">" };
    juce::TextButton lcdInQuickBtn { "IN: SIN ASIGNAR" };
    juce::TextButton lcdOutQuickBtn { "OUT: ---" };

    juce::Viewport tabViewport;
    ConsoleTabCanvas tabContent;

    juce::Slider homeTrimSlider;
    juce::Slider homeBalanceSlider;
    juce::Slider homePanSlider;
    juce::Slider homeWidthSlider;
    juce::ToggleButton homePhaseToggle { "FASE" };
    juce::ToggleButton homeHpfToggle { "HPF" };
    juce::ToggleButton homeLpfToggle { "LPF" };
    juce::ToggleButton homeEqToggle { "EQ ON" };
    juce::ToggleButton homeGateToggle { "GATE" };
    juce::ToggleButton homeCompToggle { "COMP" };
    EqCurveDisplay homeEqCurve;
    ChannelMiniMeter homeMiniMeter;
    juce::TextButton homeBrowseProfilesBtn { "Explorar Perfiles (23)" };
    juce::TextButton homeLoadProfileBtn { "Cargar JSON..." };
    juce::TextButton homeSaveProfileBtn { "Guardar JSON..." };

    juce::TextEditor srcNameEditor;
    std::array<juce::TextButton, 16> srcColorBtns;
    juce::ComboBox srcDeviceBox;
    juce::ComboBox srcModeBox;
    juce::TextButton srcRefreshDevBtn { "Refrescar" };
    juce::Label srcInputStatusLabel;
    juce::TextButton srcLoadMediaBtn { "Cargar Audio (WAV/MP3/FLAC/OGG)..." };
    juce::TextButton srcMediaPlayBtn { "Play / Pause" };
    juce::TextButton srcMediaStopBtn { "Stop" };
    juce::ToggleButton srcMediaLoopToggle { "Loop" };
    juce::Label srcMediaInfoLabel;

    juce::ToggleButton eqOnToggle { "EQ ON" };
    juce::ToggleButton eqHpfToggle { "LCUT (HPF)" };
    juce::Slider eqHpfSlider;
    juce::ToggleButton eqLpfToggle { "HCUT (LPF)" };
    juce::Slider eqLpfSlider;
    juce::TextButton eqOpenProfilesBtn { "Perfiles DSP..." };
    juce::TextButton eqResetFlatBtn { "Reset Flat" };
    EqCurveDisplay eqMainCurve;
    struct EqBandUi
    {
        juce::ToggleButton enabledToggle;
        juce::Slider freqSlider;
        juce::Slider gainSlider;
        juce::Slider qSlider;
        EqBandSatMeter satMeter;
    };
    std::array<EqBandUi, 4> eqBands;

    DynamicsCurveDisplay dynCurve;
    juce::ToggleButton dynGateToggle { "Noise Gate" };
    juce::Slider dynGateThr;
    juce::ToggleButton dynCompToggle { "Compresor" };
    juce::Slider dynCompThr;
    juce::Slider dynCompRatio;
    juce::Slider dynCompMakeup;
    juce::ToggleButton dynDeessToggle { "De-Esser" };
    juce::Slider dynDeessFreq;
    juce::Slider dynDeessThr;
    juce::ToggleButton dynDriveToggle { "Analog Drive" };
    juce::Slider dynDriveAmt;
    juce::ToggleButton dynDelayToggle { "Stereo Delay" };
    juce::Slider dynDelayTime;
    juce::Slider dynDelayFb;
    juce::Slider dynDelayMix;
    juce::ToggleButton dynRevToggle { "Studio Reverb" };
    juce::Slider dynRevRoom;
    juce::Slider dynRevDamp;
    juce::Slider dynRevMix;
    juce::ToggleButton dynLimToggle { "Peak Limiter" };
    juce::Slider dynLimCeil;

    juce::TextButton profLoadJsonBtn { "Cargar desde Archivo JSON..." };
    juce::TextButton profSaveJsonBtn { "Guardar Perfil Actual..." };
    juce::TextButton profResetFlatBtn { "Restablecer a Flat" };
    std::vector<std::unique_ptr<juce::Component>> profileRows;

    std::array<juce::TextButton, 4> vstSlotBtns;
    juce::Label vstWineWarningLabel;
    juce::TextButton vstScanBtn { "Escanear VST" };
    juce::TextButton vstBrowseBtn { "Cargar .vst3 / .so / .dll..." };
    juce::TextButton vstUnloadBtn { "Descargar" };
    juce::TextButton vstOpenGuiBtn { "Abrir GUI Nativa" };
    juce::ToggleButton vstBypassToggle { "Bypass" };
    juce::Slider vstMixSlider;
    juce::ComboBox vstDiscoveredBox;
    juce::TextButton vstLoadSelectedBtn { "Cargar Seleccionado" };
    juce::TextEditor vstParamFilter;
    juce::Viewport vstParamsViewport;
    juce::Component vstParamsContent;
    std::vector<std::unique_ptr<juce::Component>> vstParamRows;

    juce::ComboBox ioOutDeviceBox;
    juce::ComboBox ioSampleRateBox;
    juce::ComboBox ioBlockSizeBox;
    juce::TextButton ioRefreshBtn { "Refrescar E/S" };
    juce::TextButton ioStartStopBtn { "INICIAR AUDIO" };
    juce::ComboBox ioInDeviceBox;
    juce::ComboBox ioInModeBox;
    juce::Label ioRoutingStatusLabel;
    juce::ToggleButton ioVmicToggle { "Mic Virtual (Discord / OBS)" };
    juce::Slider ioVmicGainSlider;
    juce::TextButton ioRecToggleBtn { "GRABAR MULTIPISTA (REC)" };
    juce::TextButton ioOpenRecDirBtn { "Abrir Carpeta Grabaciones" };
    juce::Label ioRecDirLabel;

    juce::ComboBox setLangBox;
    juce::Label setPathsLabel;
    juce::TextButton setOpenVstDirBtn { "Abrir Carpeta VST" };
    juce::TextButton setChooseVstDirBtn { "Carpeta VST Personalizada..." };
    juce::ToggleButton setSysVstToggle { "Incluir rutas VST del sistema" };
    juce::Label setWineStatusLabel;
    juce::TextButton setWineRefreshBtn { "Inspeccionar Wine" };
    juce::TextButton setWineInitBtn { "Inicializar Prefijo" };
    juce::TextButton setWineRebuildBtn { "Compilar Puente (wineg++)" };
    juce::TextButton setWineCfgBtn { "winecfg" };
    juce::TextButton setWineExplorerBtn { "Explorer" };
    juce::TextButton setWineRunExeBtn { "Ejecutar Instalador .exe..." };

    juce::Label newsBodyLabel;

    juce::TextButton monStartBtn { "AUDIO OFF" };
    juce::TextButton monRecBtn { "REC" };
    juce::TextButton monVmicBtn { "VMIC" };
    juce::TextButton monClearSoloBtn { "CLR SOLO" };
    juce::TextButton monPlayAllBtn { "PLAY MEDIA" };
    juce::Rectangle<float> monMasterMeterL;
    juce::Rectangle<float> monMasterMeterR;
    juce::Rectangle<float> monVmicMeter;

    std::array<juce::TextButton, 5> bankBtns;
    juce::TextButton addChBtn { "+ Canal" };
    juce::TextButton addMediaBtn { "+ Media" };

    juce::Viewport stripsViewport;
    juce::Component stripsContent;
    struct ChannelStripWidget : public juce::Component
    {
        uint32_t chIdx = 0;
        juce::Colour chColour { 0xff14161c };
        juce::String chName;
        juce::String chSourceBadge;
        bool hasInputAssigned = false;
        bool isMediaChannel = false;
        bool selected = false;
        float meterL = 0.0f;
        float meterR = 0.0f;
        juce::Slider fader;
        juce::TextButton muteBtn { "M" };
        juce::TextButton soloBtn { "S" };
        juce::TextButton recBtn { "R" };
        juce::TextButton fxBtn { "FX" };
        std::function<void(uint32_t)> onSelect;
        std::function<void(uint32_t, float)> onFader;
        std::function<void(uint32_t)> onToggleMute;
        std::function<void(uint32_t)> onToggleSolo;
        std::function<void(uint32_t)> onToggleRec;
        std::function<void(uint32_t)> onOpenFx;
        ChannelStripWidget();
        void paint(juce::Graphics& g) override;
        void mouseDown(const juce::MouseEvent& e) override;
        void resized() override;
    };
    std::vector<std::unique_ptr<ChannelStripWidget>> channelStrips;

    juce::Slider masterFader;
    juce::TextButton masterMuteBtn { "MUTE" };
    juce::TextButton masterDimBtn { "DIM" };
    juce::TextButton masterRecMixBtn { "REC MIX" };
    juce::TextButton masterFxBtn { "MASTER FX" };
    juce::Rectangle<float> masterStripBounds;
    juce::Rectangle<float> masterMeterLRect;
    juce::Rectangle<float> masterMeterRRect;

    std::unique_ptr<juce::FileChooser> fileChooser;

    void setActiveTab(MixerTab tab);
    void populateSelectedChannelControls();
    void pushSelectedDspChanges();
    void rebuildProfileCards();
    void rebuildVstParams();
    void rebuildChannelStrips();
    void layoutActiveTabContent(int contentW);
    void refreshAudioDevices();
    void applyInputAssignmentFromBoxes(juce::ComboBox& devBox, juce::ComboBox& modeBox, bool changedDevice);
    void applyOutputSelection();
    void populateInputModeBox(juce::ComboBox& box);
    void populateInputDeviceBox(juce::ComboBox& box, const juce::String& selectedDevId);
    static int modeValueToComboId(int modeVal);
    static uint8_t comboIdToModeValue(int comboId);
    static juce::String modeShortLabel(int modeVal);
    juce::var getSelectedChannelVar() const;
    void notifyMsg(const juce::String& msg);
};
