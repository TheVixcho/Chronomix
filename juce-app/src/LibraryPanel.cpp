#include "LibraryPanel.h"

LibraryPanel::LibraryPanel(ChronomixBridge& bridgeRef)
    : bridge(bridgeRef)
{
    auto paths = bridge.getPaths();
    scanDir = paths["recordings_dir"].toString();
    dirLabel.setText("CARPETA: " + scanDir, juce::dontSendNotification);
    dirLabel.setFont(juce::FontOptions(11.5f));
    dirLabel.setColour(juce::Label::textColourId, StudioColors::textSecond);
    addAndMakeVisible(dirLabel);

    changeDirBtn.onClick = [this]
    {
        fileChooser = std::make_unique<juce::FileChooser>("Elegir carpeta de biblioteca de audio", juce::File(scanDir));
        fileChooser->launchAsync(juce::FileBrowserComponent::openMode | juce::FileBrowserComponent::canSelectDirectories,
                                 [this](const juce::FileChooser& fc)
                                 {
                                     auto f = fc.getResult();
                                     if (f.isDirectory())
                                     {
                                         scanDir = f.getFullPathName();
                                         dirLabel.setText("CARPETA: " + scanDir, juce::dontSendNotification);
                                         refreshFiles();
                                     }
                                 });
    };
    addAndMakeVisible(changeDirBtn);

    openDirBtn.onClick = [this]
    {
        juce::File(scanDir).revealToUser();
    };
    addAndMakeVisible(openDirBtn);

    refreshBtn.onClick = [this] { refreshFiles(); };
    addAndMakeVisible(refreshBtn);

    searchBox.setTextToShowWhenEmpty("Buscar archivo de audio o sesión...", StudioColors::textMuted);
    searchBox.onTextChange = [this] { applyFilter(); };
    addAndMakeVisible(searchBox);

    listBox.setColour(juce::ListBox::backgroundColourId, StudioColors::bgScreen);
    listBox.setRowHeight(28);
    addAndMakeVisible(listBox);

    detailTitle.setFont(juce::FontOptions(14.0f, juce::Font::bold));
    detailTitle.setColour(juce::Label::textColourId, StudioColors::orange);
    addAndMakeVisible(detailTitle);

    detailMeta.setFont(juce::FontOptions(12.0f));
    detailMeta.setColour(juce::Label::textColourId, StudioColors::textSecond);
    detailMeta.setJustificationType(juce::Justification::topLeft);
    addAndMakeVisible(detailMeta);

    assignMixerBtn.setColour(juce::TextButton::buttonColourId, StudioColors::orange);
    assignMixerBtn.setColour(juce::TextButton::textColourOffId, juce::Colours::black);
    assignMixerBtn.onClick = [this]
    {
        int row = listBox.getSelectedRow();
        if (row >= 0 && row < static_cast<int>(filteredFiles.size()) && onAssignToMixer)
            onAssignToMixer(juce::File(filteredFiles[static_cast<size_t>(row)].path));
    };
    addAndMakeVisible(assignMixerBtn);

    loadDjABtn.setColour(juce::TextButton::buttonColourId, StudioColors::cyan.withAlpha(0.30f));
    loadDjABtn.onClick = [this]
    {
        int row = listBox.getSelectedRow();
        if (row >= 0 && row < static_cast<int>(filteredFiles.size()) && onAssignToDjDeck)
            onAssignToDjDeck(0, juce::File(filteredFiles[static_cast<size_t>(row)].path));
    };
    addAndMakeVisible(loadDjABtn);

    loadDjBBtn.setColour(juce::TextButton::buttonColourId, StudioColors::orange.withAlpha(0.30f));
    loadDjBBtn.onClick = [this]
    {
        int row = listBox.getSelectedRow();
        if (row >= 0 && row < static_cast<int>(filteredFiles.size()) && onAssignToDjDeck)
            onAssignToDjDeck(1, juce::File(filteredFiles[static_cast<size_t>(row)].path));
    };
    addAndMakeVisible(loadDjBBtn);

    refreshFiles();
}

void LibraryPanel::refreshFiles()
{
    allFiles.clear();
    auto v = ChronomixBridge::takeJson(chronomix_library_scan_json(scanDir.toRawUTF8()));
    if (auto* arr = v.getArray())
    {
        for (const auto& item : *arr)
        {
            LibraryItem li;
            li.path = item["path"].toString();
            li.filename = item["filename"].toString();
            li.parentFolder = item["parent_folder"].toString();
            li.extension = item["extension"].toString();
            li.sizeBytes = static_cast<int64_t>(static_cast<juce::int64>(item["size_bytes"]));
            allFiles.push_back(std::move(li));
        }
    }
    applyFilter();
}

void LibraryPanel::applyFilter()
{
    auto q = searchBox.getText().trim().toLowerCase();
    filteredFiles.clear();
    for (const auto& f : allFiles)
    {
        if (q.isEmpty()
            || f.filename.toLowerCase().contains(q)
            || f.parentFolder.toLowerCase().contains(q))
        {
            filteredFiles.push_back(f);
        }
    }
    listBox.updateContent();
    if (!filteredFiles.empty() && listBox.getSelectedRow() < 0)
        listBox.selectRow(0);
    updateDetail();
}

void LibraryPanel::updateDetail()
{
    int row = listBox.getSelectedRow();
    if (row < 0 || row >= static_cast<int>(filteredFiles.size()))
    {
        detailTitle.setText("Biblioteca de Audio", juce::dontSendNotification);
        detailMeta.setText("Selecciona un archivo de audio para asignarlo al Mixer o cargarlo directamente en el panel de DJ (Deck A o Deck B).\n\nLas grabaciones multipista se guardan automáticamente en:\n" + scanDir, juce::dontSendNotification);
        assignMixerBtn.setEnabled(false);
        loadDjABtn.setEnabled(false);
        loadDjBBtn.setEnabled(false);
        return;
    }

    const auto& f = filteredFiles[static_cast<size_t>(row)];
    double mb = static_cast<double>(f.sizeBytes) / (1024.0 * 1024.0);
    detailTitle.setText(f.filename, juce::dontSendNotification);
    detailMeta.setText("Formato: " + f.extension + "\nTamaño: " + juce::String(mb, 2) + " MB\nCarpeta: " + f.parentFolder + "\nRuta: " + f.path, juce::dontSendNotification);
    assignMixerBtn.setEnabled(true);
    loadDjABtn.setEnabled(true);
    loadDjBBtn.setEnabled(true);
}

int LibraryPanel::getNumRows() { return static_cast<int>(filteredFiles.size()); }

void LibraryPanel::paintListBoxItem(int rowNumber, juce::Graphics& g, int width, int height, bool rowIsSelected)
{
    if (rowNumber < 0 || rowNumber >= static_cast<int>(filteredFiles.size()))
        return;
    const auto& f = filteredFiles[static_cast<size_t>(rowNumber)];
    g.setColour(rowIsSelected ? StudioColors::bgStripSel : ((rowNumber % 2 == 0) ? StudioColors::bgScreen : StudioColors::bgBezel));
    g.fillRect(0, 0, width, height);
    if (rowIsSelected)
    {
        g.setColour(StudioColors::orange);
        g.fillRect(0, 0, 3, height);
    }
    g.setColour(StudioColors::cyan);
    g.setFont(juce::FontOptions(10.0f, juce::Font::bold));
    g.drawText(f.extension, 8, 0, 42, height, juce::Justification::centredLeft);

    g.setColour(StudioColors::textPrimary);
    g.setFont(juce::FontOptions(12.0f));
    g.drawFittedText(f.filename, 54, 0, width - 160, height, juce::Justification::centredLeft, 1);

    g.setColour(StudioColors::textMuted);
    g.setFont(juce::FontOptions(10.5f));
    g.drawFittedText(f.parentFolder, width - 104, 0, 98, height, juce::Justification::centredRight, 1);
}

void LibraryPanel::selectedRowsChanged(int) { updateDetail(); }

void LibraryPanel::paint(juce::Graphics& g)
{
    g.fillAll(StudioColors::bgDeep);
}

void LibraryPanel::resized()
{
    auto area = getLocalBounds().reduced(10);
    auto topBar = area.removeFromTop(28);
    refreshBtn.setBounds(topBar.removeFromRight(84));
    topBar.removeFromRight(4);
    openDirBtn.setBounds(topBar.removeFromRight(98));
    topBar.removeFromRight(4);
    changeDirBtn.setBounds(topBar.removeFromRight(124));
    topBar.removeFromRight(8);
    dirLabel.setBounds(topBar);

    area.removeFromTop(8);
    auto leftCol = area.removeFromLeft(area.getWidth() * 60 / 100);
    area.removeFromLeft(10);
    auto rightCol = area;

    searchBox.setBounds(leftCol.removeFromTop(26));
    leftCol.removeFromTop(6);
    listBox.setBounds(leftCol);

    detailTitle.setBounds(rightCol.removeFromTop(28));
    rightCol.removeFromTop(6);
    detailMeta.setBounds(rightCol.removeFromTop(140));
    rightCol.removeFromTop(12);
    assignMixerBtn.setBounds(rightCol.removeFromTop(32));
    rightCol.removeFromTop(8);
    auto djRow = rightCol.removeFromTop(32);
    loadDjABtn.setBounds(djRow.removeFromLeft(djRow.getWidth() / 2 - 3));
    djRow.removeFromLeft(6);
    loadDjBBtn.setBounds(djRow);
}
