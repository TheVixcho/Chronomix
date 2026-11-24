#pragma once

#include <juce_gui_basics/juce_gui_basics.h>
#include "FfiBridge.h"
#include "ChronomixLookAndFeel.h"
#include <functional>
#include <vector>

struct LibraryItem
{
    juce::String path;
    juce::String filename;
    juce::String parentFolder;
    juce::String extension;
    int64_t sizeBytes = 0;
};

class LibraryPanel : public juce::Component,
                     public juce::ListBoxModel
{
public:
    LibraryPanel(ChronomixBridge& bridgeRef);
    void refreshFiles();

    std::function<void(const juce::File&)> onAssignToMixer;
    std::function<void(uint32_t, const juce::File&)> onAssignToDjDeck;

    void paint(juce::Graphics& g) override;
    void resized() override;

    int getNumRows() override;
    void paintListBoxItem(int rowNumber, juce::Graphics& g, int width, int height, bool rowIsSelected) override;
    void selectedRowsChanged(int lastRowSelected) override;

private:
    ChronomixBridge& bridge;
    juce::String scanDir;
    std::vector<LibraryItem> allFiles;
    std::vector<LibraryItem> filteredFiles;

    juce::Label dirLabel;
    juce::TextButton changeDirBtn { "Cambiar Carpeta..." };
    juce::TextButton openDirBtn { "Abrir Carpeta" };
    juce::TextButton refreshBtn { "Actualizar" };
    juce::TextEditor searchBox;
    juce::ListBox listBox { "LibraryList", this };

    juce::Label detailTitle;
    juce::Label detailMeta;
    juce::TextButton assignMixerBtn { "Asignar a Canal Mixer" };
    juce::TextButton loadDjABtn { "Cargar en DJ Deck A" };
    juce::TextButton loadDjBBtn { "Cargar en DJ Deck B" };

    std::unique_ptr<juce::FileChooser> fileChooser;
    void applyFilter();
    void updateDetail();
};
