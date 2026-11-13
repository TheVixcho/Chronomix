#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <ole2.h>

#include <fcntl.h>
#include <poll.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <algorithm>
#include <atomic>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <sstream>
#include <string>
#include <vector>

namespace vst3 {

using tresult = int32_t;
using uint32 = uint32_t;
using int32 = int32_t;
using int16 = int16_t;
using int64 = int64_t;
using uint64 = uint64_t;
using TBool = uint8_t;
using TChar = char16_t;
using FIDString = const char*;
using ParamID = uint32_t;
using ParamValue = double;
using MediaType = int32_t;
using BusDirection = int32_t;
using BusType = int32_t;
using IoMode = int32_t;
using SpeakerArrangement = uint64_t;
using UnitID = int32_t;
using String128 = TChar[128];

struct TUID {
    int8_t data[16];
};

constexpr tresult kResultOk = 0;
constexpr tresult kResultTrue = 0;
constexpr tresult kResultFalse = 1;
constexpr tresult kNoInterface = static_cast<tresult>(0x80004002);
constexpr tresult kNotImplemented = static_cast<tresult>(0x80004001);
constexpr tresult kInvalidArgument = static_cast<tresult>(0x80070057);

constexpr int32 kMediaAudio = 0;
constexpr int32 kDirInput = 0;
constexpr int32 kDirOutput = 1;
constexpr int32 kRealtime = 0;
constexpr int32 kSample32 = 0;

constexpr TUID make_win_uid(uint32_t a, uint32_t b, uint32_t c, uint32_t d) {
    return TUID{{
        static_cast<int8_t>((a & 0x000000FF) >> 0),
        static_cast<int8_t>((a & 0x0000FF00) >> 8),
        static_cast<int8_t>((a & 0x00FF0000) >> 16),
        static_cast<int8_t>((a & 0xFF000000) >> 24),
        static_cast<int8_t>((b & 0x00FF0000) >> 16),
        static_cast<int8_t>((b & 0xFF000000) >> 24),
        static_cast<int8_t>((b & 0x000000FF) >> 0),
        static_cast<int8_t>((b & 0x0000FF00) >> 8),
        static_cast<int8_t>((c & 0xFF000000) >> 24),
        static_cast<int8_t>((c & 0x00FF0000) >> 16),
        static_cast<int8_t>((c & 0x0000FF00) >> 8),
        static_cast<int8_t>((c & 0x000000FF) >> 0),
        static_cast<int8_t>((d & 0xFF000000) >> 24),
        static_cast<int8_t>((d & 0x00FF0000) >> 16),
        static_cast<int8_t>((d & 0x0000FF00) >> 8),
        static_cast<int8_t>((d & 0x000000FF) >> 0),
    }};
}

inline bool tuid_eq(const TUID* a, const TUID& b) {
    return std::memcmp(a, &b, 16) == 0;
}

constexpr TUID IID_FUnknown = make_win_uid(0x00000000, 0x00000000, 0xC0000000, 0x00000046);
constexpr TUID IID_IPluginFactory = make_win_uid(0x7A4D811C, 0x52114A1F, 0xAED9D2EE, 0x0B43BF9F);
constexpr TUID IID_IPluginFactory2 = make_win_uid(0x0007B650, 0xF24B4C0B, 0xA464EDB9, 0xF00B2ABB);
constexpr TUID IID_IComponent = make_win_uid(0xE831FF31, 0xF2D54301, 0x928EBBEE, 0x25697802);
constexpr TUID IID_IAudioProcessor = make_win_uid(0x42043F99, 0xB7DA453C, 0xA569E79D, 0x9AAEC33D);
constexpr TUID IID_IEditController = make_win_uid(0xDCD7BBE3, 0x7742448D, 0xA874AACC, 0x979C759E);
constexpr TUID IID_IComponentHandler = make_win_uid(0x93A0BEA3, 0x0BD045DB, 0x8E890B0C, 0xC1E46AC6);
constexpr TUID IID_IConnectionPoint = make_win_uid(0x70A4156F, 0x6E6E4026, 0x989148BF, 0xAA60D8D1);
constexpr TUID IID_IPlugView = make_win_uid(0x5BC32507, 0xD06049EA, 0xA6151B52, 0x2B755B29);
constexpr TUID IID_IPlugFrame = make_win_uid(0x367FAF01, 0xAFA94693, 0x8D4DA2A0, 0xED0882A3);
constexpr TUID IID_IHostApplication = make_win_uid(0x58E595CC, 0xDB2D4969, 0x8B6AAF8C, 0x36A664E5);
constexpr TUID IID_IParameterChanges = make_win_uid(0xA4779663, 0x0BB64A56, 0xB44384A8, 0x466FEB9D);
constexpr TUID IID_IParamValueQueue = make_win_uid(0x01263A18, 0xED074F6F, 0x98C9D356, 0x4686F9BA);
constexpr TUID IID_IBStream = make_win_uid(0xC3BF6EA2, 0x30994752, 0x9B6BF990, 0x1EE33E9B);
constexpr TUID IID_IEventList = make_win_uid(0x3A2C4214, 0x346349FE, 0xB2C4F397, 0xB9695A44);

struct ViewRect {
    int32 left;
    int32 top;
    int32 right;
    int32 bottom;
};

struct PFactoryInfo {
    char vendor[64];
    char url[256];
    char email[128];
    int32 flags;
};

struct PClassInfo {
    TUID cid;
    int32 cardinality;
    char category[32];
    char name[64];
};

struct PClassInfo2 {
    TUID cid;
    int32 cardinality;
    char category[32];
    char name[64];
    uint32 classFlags;
    char subCategories[128];
    char vendor[64];
    char version[64];
    char sdkVersion[64];
};

struct BusInfo {
    MediaType mediaType;
    BusDirection direction;
    int32 channelCount;
    String128 name;
    BusType busType;
    uint32 flags;
};

struct RoutingInfo;
struct IBStream;
struct IMessage;
struct IEventList;

struct ParameterInfo {
    ParamID id;
    String128 title;
    String128 shortTitle;
    String128 units;
    int32 stepCount;
    ParamValue defaultNormalizedValue;
    UnitID unitId;
    int32 flags;
};

struct AudioBusBuffers {
    int32 numChannels;
    uint64 silenceFlags;
    union {
        float** channelBuffers32;
        double** channelBuffers64;
    };
};

struct ProcessSetup {
    int32 processMode;
    int32 symbolicSampleSize;
    int32 maxSamplesPerBlock;
    double sampleRate;
};

struct ProcessContext {
    uint32 state;
    double sampleRate;
    int64 projectTimeSamples;
    int64 systemTime;
    int64 continousTimeSamples;
    double projectTimeMusic;
    double barPositionMusic;
    double cycleStartMusic;
    double cycleEndMusic;
    double tempo;
    int32 timeSigNumerator;
    int32 timeSigDenominator;
    uint8_t chord[4];
    int32 smpteOffsetSubframes;
    uint8_t frameRate[8];
    int32 samplesToNextClock;
};

struct FUnknown {
    virtual tresult STDMETHODCALLTYPE queryInterface(const TUID* iid, void** obj) = 0;
    virtual uint32 STDMETHODCALLTYPE addRef() = 0;
    virtual uint32 STDMETHODCALLTYPE release() = 0;
};

struct IBStream : public FUnknown {
    virtual tresult STDMETHODCALLTYPE read(void* buffer, int32 numBytes, int32* numBytesRead) = 0;
    virtual tresult STDMETHODCALLTYPE write(void* buffer, int32 numBytes, int32* numBytesWritten) = 0;
    virtual tresult STDMETHODCALLTYPE seek(int64 pos, int32 mode, int64* result) = 0;
    virtual tresult STDMETHODCALLTYPE tell(int64* pos) = 0;
};

struct EventStub {
    int32 busIndex;
    int32 sampleOffset;
    double ppqPosition;
    uint16_t flags;
    uint16_t type;
    uint8_t payload[32];
};

struct IEventList : public FUnknown {
    virtual int32 STDMETHODCALLTYPE getEventCount() = 0;
    virtual tresult STDMETHODCALLTYPE getEvent(int32 index, EventStub* e) = 0;
    virtual tresult STDMETHODCALLTYPE addEvent(EventStub* e) = 0;
};

struct IPluginBase : public FUnknown {
    virtual tresult STDMETHODCALLTYPE initialize(FUnknown* context) = 0;
    virtual tresult STDMETHODCALLTYPE terminate() = 0;
};

struct IPluginFactory : public FUnknown {
    virtual tresult STDMETHODCALLTYPE getFactoryInfo(PFactoryInfo* info) = 0;
    virtual int32 STDMETHODCALLTYPE countClasses() = 0;
    virtual tresult STDMETHODCALLTYPE getClassInfo(int32 index, PClassInfo* info) = 0;
    virtual tresult STDMETHODCALLTYPE createInstance(const char* cid, const char* iid, void** obj) = 0;
};

struct IPluginFactory2 : public IPluginFactory {
    virtual tresult STDMETHODCALLTYPE getClassInfo2(int32 index, PClassInfo2* info) = 0;
};

struct IComponent : public IPluginBase {
    virtual tresult STDMETHODCALLTYPE getControllerClassId(TUID* classId) = 0;
    virtual tresult STDMETHODCALLTYPE setIoMode(IoMode mode) = 0;
    virtual int32 STDMETHODCALLTYPE getBusCount(MediaType type, BusDirection dir) = 0;
    virtual tresult STDMETHODCALLTYPE getBusInfo(MediaType type, BusDirection dir, int32 index, BusInfo* bus) = 0;
    virtual tresult STDMETHODCALLTYPE getRoutingInfo(RoutingInfo* inInfo, RoutingInfo* outInfo) = 0;
    virtual tresult STDMETHODCALLTYPE activateBus(MediaType type, BusDirection dir, int32 index, TBool state) = 0;
    virtual tresult STDMETHODCALLTYPE setActive(TBool state) = 0;
    virtual tresult STDMETHODCALLTYPE setState(IBStream* state) = 0;
    virtual tresult STDMETHODCALLTYPE getState(IBStream* state) = 0;
};

struct IParamValueQueue : public FUnknown {
    virtual ParamID STDMETHODCALLTYPE getParameterId() = 0;
    virtual int32 STDMETHODCALLTYPE getPointCount() = 0;
    virtual tresult STDMETHODCALLTYPE getPoint(int32 index, int32* sampleOffset, ParamValue* value) = 0;
    virtual tresult STDMETHODCALLTYPE addPoint(int32 sampleOffset, ParamValue value, int32* index) = 0;
};

struct IParameterChanges : public FUnknown {
    virtual int32 STDMETHODCALLTYPE getParameterCount() = 0;
    virtual IParamValueQueue* STDMETHODCALLTYPE getParameterData(int32 index) = 0;
    virtual IParamValueQueue* STDMETHODCALLTYPE addParameterData(const ParamID* id, int32* index) = 0;
};

struct ProcessData {
    int32 processMode;
    int32 symbolicSampleSize;
    int32 numSamples;
    int32 numInputs;
    int32 numOutputs;
    AudioBusBuffers* inputs;
    AudioBusBuffers* outputs;
    IParameterChanges* inputParameterChanges;
    IParameterChanges* outputParameterChanges;
    IEventList* inputEvents;
    IEventList* outputEvents;
    ProcessContext* processContext;
};

struct IAudioProcessor : public FUnknown {
    virtual tresult STDMETHODCALLTYPE setBusArrangements(SpeakerArrangement* inputs, int32 numIns, SpeakerArrangement* outputs, int32 numOuts) = 0;
    virtual tresult STDMETHODCALLTYPE getBusArrangement(BusDirection dir, int32 index, SpeakerArrangement* arr) = 0;
    virtual tresult STDMETHODCALLTYPE canProcessSampleSize(int32 symbolicSampleSize) = 0;
    virtual uint32 STDMETHODCALLTYPE getLatencySamples() = 0;
    virtual tresult STDMETHODCALLTYPE setupProcessing(ProcessSetup* setup) = 0;
    virtual tresult STDMETHODCALLTYPE setProcessing(TBool state) = 0;
    virtual tresult STDMETHODCALLTYPE process(ProcessData* data) = 0;
    virtual uint32 STDMETHODCALLTYPE getTailSamples() = 0;
};

struct IComponentHandler : public FUnknown {
    virtual tresult STDMETHODCALLTYPE beginEdit(ParamID id) = 0;
    virtual tresult STDMETHODCALLTYPE performEdit(ParamID id, ParamValue valueNormalized) = 0;
    virtual tresult STDMETHODCALLTYPE endEdit(ParamID id) = 0;
    virtual tresult STDMETHODCALLTYPE restartComponent(int32 flags) = 0;
};

struct IPlugView;

struct IPlugFrame : public FUnknown {
    virtual tresult STDMETHODCALLTYPE resizeView(IPlugView* view, ViewRect* newSize) = 0;
};

struct IPlugView : public FUnknown {
    virtual tresult STDMETHODCALLTYPE isPlatformTypeSupported(FIDString type) = 0;
    virtual tresult STDMETHODCALLTYPE attached(void* parent, FIDString type) = 0;
    virtual tresult STDMETHODCALLTYPE removed() = 0;
    virtual tresult STDMETHODCALLTYPE onWheel(float distance) = 0;
    virtual tresult STDMETHODCALLTYPE onKeyDown(TChar key, int16 keyCode, int16 modifiers) = 0;
    virtual tresult STDMETHODCALLTYPE onKeyUp(TChar key, int16 keyCode, int16 modifiers) = 0;
    virtual tresult STDMETHODCALLTYPE getSize(ViewRect* size) = 0;
    virtual tresult STDMETHODCALLTYPE onSize(ViewRect* newSize) = 0;
    virtual tresult STDMETHODCALLTYPE onFocus(TBool state) = 0;
    virtual tresult STDMETHODCALLTYPE setFrame(IPlugFrame* frame) = 0;
    virtual tresult STDMETHODCALLTYPE canResize() = 0;
    virtual tresult STDMETHODCALLTYPE checkSizeConstraint(ViewRect* rect) = 0;
};

struct IEditController : public IPluginBase {
    virtual tresult STDMETHODCALLTYPE setComponentState(IBStream* state) = 0;
    virtual tresult STDMETHODCALLTYPE setState(IBStream* state) = 0;
    virtual tresult STDMETHODCALLTYPE getState(IBStream* state) = 0;
    virtual int32 STDMETHODCALLTYPE getParameterCount() = 0;
    virtual tresult STDMETHODCALLTYPE getParameterInfo(int32 paramIndex, ParameterInfo* info) = 0;
    virtual tresult STDMETHODCALLTYPE getParamStringByValue(ParamID id, ParamValue valueNormalized, String128* string) = 0;
    virtual tresult STDMETHODCALLTYPE getParamValueByString(ParamID id, TChar* string, ParamValue* valueNormalized) = 0;
    virtual ParamValue STDMETHODCALLTYPE normalizedParamToPlain(ParamID id, ParamValue valueNormalized) = 0;
    virtual ParamValue STDMETHODCALLTYPE plainParamToNormalized(ParamID id, ParamValue plainValue) = 0;
    virtual ParamValue STDMETHODCALLTYPE getParamNormalized(ParamID id) = 0;
    virtual tresult STDMETHODCALLTYPE setParamNormalized(ParamID id, ParamValue value) = 0;
    virtual tresult STDMETHODCALLTYPE setComponentHandler(IComponentHandler* handler) = 0;
    virtual IPlugView* STDMETHODCALLTYPE createView(FIDString name) = 0;
};

struct IConnectionPoint : public FUnknown {
    virtual tresult STDMETHODCALLTYPE connect(IConnectionPoint* other) = 0;
    virtual tresult STDMETHODCALLTYPE disconnect(IConnectionPoint* other) = 0;
    virtual tresult STDMETHODCALLTYPE notify(IMessage* message) = 0;
};

struct IHostApplication : public FUnknown {
    virtual tresult STDMETHODCALLTYPE getName(String128* name) = 0;
    virtual tresult STDMETHODCALLTYPE createInstance(TUID* cid, TUID* iid, void** obj) = 0;
};

} // namespace vst3

constexpr uint32_t SHM_MAGIC = 0x4F50454E;
constexpr size_t SHM_MAX_FRAMES = 4096;
constexpr size_t SHM_MAX_PARAMS = 128;

struct ShmAudioBlock {
    uint32_t magic;
    uint32_t max_frames;
    uint32_t num_samples;
    uint32_t pending_param_count;
    double sample_rate;
    uint32_t pending_param_ids[SHM_MAX_PARAMS];
    double pending_param_vals[SHM_MAX_PARAMS];
    float in_l[SHM_MAX_FRAMES];
    float in_r[SHM_MAX_FRAMES];
    float out_l[SHM_MAX_FRAMES];
    float out_r[SHM_MAX_FRAMES];
};

static std::string utf16_to_utf8(const char16_t* s, size_t max_len = 128) {
    if (!s) return {};
    size_t len = 0;
    while (len < max_len && s[len] != 0) ++len;
    if (len == 0) return {};
    int needed = WideCharToMultiByte(
        CP_UTF8, 0, reinterpret_cast<const wchar_t*>(s), static_cast<int>(len), nullptr, 0, nullptr, nullptr);
    if (needed <= 0) return {};
    std::string out(static_cast<size_t>(needed), '\0');
    WideCharToMultiByte(
        CP_UTF8, 0, reinterpret_cast<const wchar_t*>(s), static_cast<int>(len), out.data(), needed, nullptr, nullptr);
    return out;
}

static std::string json_escape(const std::string& s) {
    std::string out;
    out.reserve(s.size() + 8);
    for (unsigned char c : s) {
        switch (c) {
            case '\"': out += "\\\""; break;
            case '\\': out += "\\\\"; break;
            case '\b': out += "\\b"; break;
            case '\f': out += "\\f"; break;
            case '\n': out += "\\n"; break;
            case '\r': out += "\\r"; break;
            case '\t': out += "\\t"; break;
            default:
                if (c < 0x20) {
                    char buf[8];
                    std::snprintf(buf, sizeof(buf), "\\u%04x", c);
                    out += buf;
                } else {
                    out += static_cast<char>(c);
                }
        }
    }
    return out;
}

class HostParamQueue final : public vst3::IParamValueQueue {
public:
    vst3::ParamID id = 0;
    double value = 0.0;
    bool active = false;

    vst3::tresult STDMETHODCALLTYPE queryInterface(const vst3::TUID* iid, void** obj) override {
        if (vst3::tuid_eq(iid, vst3::IID_FUnknown) || vst3::tuid_eq(iid, vst3::IID_IParamValueQueue)) {
            *obj = static_cast<vst3::IParamValueQueue*>(this);
            addRef();
            return vst3::kResultOk;
        }
        *obj = nullptr;
        return vst3::kNoInterface;
    }
    vst3::uint32 STDMETHODCALLTYPE addRef() override { return 1; }
    vst3::uint32 STDMETHODCALLTYPE release() override { return 1; }
    vst3::ParamID STDMETHODCALLTYPE getParameterId() override { return id; }
    vst3::int32 STDMETHODCALLTYPE getPointCount() override { return active ? 1 : 0; }
    vst3::tresult STDMETHODCALLTYPE getPoint(vst3::int32 index, vst3::int32* sampleOffset, vst3::ParamValue* val) override {
        if (!active || index != 0 || !sampleOffset || !val) return vst3::kInvalidArgument;
        *sampleOffset = 0;
        *val = value;
        return vst3::kResultOk;
    }
    vst3::tresult STDMETHODCALLTYPE addPoint(vst3::int32, vst3::ParamValue val, vst3::int32* index) override {
        value = val;
        active = true;
        if (index) *index = 0;
        return vst3::kResultOk;
    }
};

class HostParamChanges final : public vst3::IParameterChanges {
public:
    HostParamQueue queues[SHM_MAX_PARAMS];
    int32_t count = 0;

    void clear() {
        for (int32_t i = 0; i < count; ++i) queues[i].active = false;
        count = 0;
    }

    void push(uint32_t id, double val) {
        for (int32_t i = 0; i < count; ++i) {
            if (queues[i].id == id) {
                queues[i].value = val;
                queues[i].active = true;
                return;
            }
        }
        if (count < static_cast<int32_t>(SHM_MAX_PARAMS)) {
            queues[count].id = id;
            queues[count].value = val;
            queues[count].active = true;
            ++count;
        }
    }

    vst3::tresult STDMETHODCALLTYPE queryInterface(const vst3::TUID* iid, void** obj) override {
        if (vst3::tuid_eq(iid, vst3::IID_FUnknown) || vst3::tuid_eq(iid, vst3::IID_IParameterChanges)) {
            *obj = static_cast<vst3::IParameterChanges*>(this);
            addRef();
            return vst3::kResultOk;
        }
        *obj = nullptr;
        return vst3::kNoInterface;
    }
    vst3::uint32 STDMETHODCALLTYPE addRef() override { return 1; }
    vst3::uint32 STDMETHODCALLTYPE release() override { return 1; }
    vst3::int32 STDMETHODCALLTYPE getParameterCount() override { return count; }
    vst3::IParamValueQueue* STDMETHODCALLTYPE getParameterData(vst3::int32 index) override {
        if (index < 0 || index >= count) return nullptr;
        return &queues[index];
    }
    vst3::IParamValueQueue* STDMETHODCALLTYPE addParameterData(const vst3::ParamID* id, vst3::int32* index) override {
        if (!id) return nullptr;
        push(*id, 0.0);
        if (index) *index = count - 1;
        return &queues[count - 1];
    }
};

class HostApplication final : public vst3::IHostApplication {
public:
    vst3::tresult STDMETHODCALLTYPE queryInterface(const vst3::TUID* iid, void** obj) override {
        if (vst3::tuid_eq(iid, vst3::IID_FUnknown) || vst3::tuid_eq(iid, vst3::IID_IHostApplication)) {
            *obj = static_cast<vst3::IHostApplication*>(this);
            addRef();
            return vst3::kResultOk;
        }
        *obj = nullptr;
        return vst3::kNoInterface;
    }
    vst3::uint32 STDMETHODCALLTYPE addRef() override { return 1; }
    vst3::uint32 STDMETHODCALLTYPE release() override { return 1; }
    vst3::tresult STDMETHODCALLTYPE getName(vst3::String128* name) override {
        if (!name) return vst3::kInvalidArgument;
        const char16_t label[] = u"Chronomix";
        std::memset(name, 0, sizeof(vst3::String128));
        std::memcpy(name, label, sizeof(label));
        return vst3::kResultOk;
    }
    vst3::tresult STDMETHODCALLTYPE createInstance(vst3::TUID*, vst3::TUID*, void** obj) override {
        if (obj) *obj = nullptr;
        return vst3::kNotImplemented;
    }
};

class HostEventList final : public vst3::IEventList {
public:
    vst3::tresult STDMETHODCALLTYPE queryInterface(const vst3::TUID* iid, void** obj) override {
        if (vst3::tuid_eq(iid, vst3::IID_FUnknown) || vst3::tuid_eq(iid, vst3::IID_IEventList)) {
            *obj = static_cast<vst3::IEventList*>(this);
            addRef();
            return vst3::kResultOk;
        }
        *obj = nullptr;
        return vst3::kNoInterface;
    }
    vst3::uint32 STDMETHODCALLTYPE addRef() override { return 1; }
    vst3::uint32 STDMETHODCALLTYPE release() override { return 1; }
    vst3::int32 STDMETHODCALLTYPE getEventCount() override { return 0; }
    vst3::tresult STDMETHODCALLTYPE getEvent(vst3::int32, vst3::EventStub*) override {
        return vst3::kInvalidArgument;
    }
    vst3::tresult STDMETHODCALLTYPE addEvent(vst3::EventStub*) override {
        return vst3::kResultOk;
    }
};

class HostMemoryStream final : public vst3::IBStream {
public:
    std::vector<uint8_t> data;
    size_t cursor = 0;

    vst3::tresult STDMETHODCALLTYPE queryInterface(const vst3::TUID* iid, void** obj) override {
        if (vst3::tuid_eq(iid, vst3::IID_FUnknown) || vst3::tuid_eq(iid, vst3::IID_IBStream)) {
            *obj = static_cast<vst3::IBStream*>(this);
            addRef();
            return vst3::kResultOk;
        }
        *obj = nullptr;
        return vst3::kNoInterface;
    }
    vst3::uint32 STDMETHODCALLTYPE addRef() override { return 1; }
    vst3::uint32 STDMETHODCALLTYPE release() override { return 1; }
    vst3::tresult STDMETHODCALLTYPE read(void* buffer, vst3::int32 numBytes, vst3::int32* numBytesRead) override {
        if (!buffer || numBytes < 0) return vst3::kInvalidArgument;
        size_t avail = cursor < data.size() ? data.size() - cursor : 0;
        size_t to_read = std::min<size_t>(avail, static_cast<size_t>(numBytes));
        if (to_read > 0) {
            std::memcpy(buffer, data.data() + cursor, to_read);
            cursor += to_read;
        }
        if (numBytesRead) *numBytesRead = static_cast<vst3::int32>(to_read);
        return vst3::kResultOk;
    }
    vst3::tresult STDMETHODCALLTYPE write(void* buffer, vst3::int32 numBytes, vst3::int32* numBytesWritten) override {
        if (!buffer || numBytes < 0) return vst3::kInvalidArgument;
        size_t n = static_cast<size_t>(numBytes);
        if (cursor + n > data.size()) data.resize(cursor + n);
        if (n > 0) std::memcpy(data.data() + cursor, buffer, n);
        cursor += n;
        if (numBytesWritten) *numBytesWritten = numBytes;
        return vst3::kResultOk;
    }
    vst3::tresult STDMETHODCALLTYPE seek(vst3::int64 pos, vst3::int32 mode, vst3::int64* result) override {
        int64_t next = 0;
        if (mode == 0) next = pos;
        else if (mode == 1) next = static_cast<int64_t>(cursor) + pos;
        else if (mode == 2) next = static_cast<int64_t>(data.size()) + pos;
        else return vst3::kInvalidArgument;
        if (next < 0) next = 0;
        cursor = static_cast<size_t>(next);
        if (result) *result = static_cast<vst3::int64>(cursor);
        return vst3::kResultOk;
    }
    vst3::tresult STDMETHODCALLTYPE tell(vst3::int64* pos) override {
        if (pos) *pos = static_cast<vst3::int64>(cursor);
        return vst3::kResultOk;
    }
};

struct GuiEdit {
    uint32_t id;
    double value;
};

static std::mutex g_edit_mutex;
static std::vector<GuiEdit> g_gui_edits;

class ComponentHandler final : public vst3::IComponentHandler {
public:
    vst3::tresult STDMETHODCALLTYPE queryInterface(const vst3::TUID* iid, void** obj) override {
        if (vst3::tuid_eq(iid, vst3::IID_FUnknown) || vst3::tuid_eq(iid, vst3::IID_IComponentHandler)) {
            *obj = static_cast<vst3::IComponentHandler*>(this);
            addRef();
            return vst3::kResultOk;
        }
        *obj = nullptr;
        return vst3::kNoInterface;
    }
    vst3::uint32 STDMETHODCALLTYPE addRef() override { return 1; }
    vst3::uint32 STDMETHODCALLTYPE release() override { return 1; }
    vst3::tresult STDMETHODCALLTYPE beginEdit(vst3::ParamID) override { return vst3::kResultOk; }
    vst3::tresult STDMETHODCALLTYPE performEdit(vst3::ParamID id, vst3::ParamValue valueNormalized) override {
        std::lock_guard<std::mutex> lock(g_edit_mutex);
        g_gui_edits.push_back(GuiEdit{id, valueNormalized});
        return vst3::kResultOk;
    }
    vst3::tresult STDMETHODCALLTYPE endEdit(vst3::ParamID) override { return vst3::kResultOk; }
    vst3::tresult STDMETHODCALLTYPE restartComponent(vst3::int32) override { return vst3::kResultOk; }
};

static HWND g_hwnd = nullptr;
static vst3::IPlugView* g_view = nullptr;
static std::atomic<bool> g_gui_open{false};

class PlugFrame final : public vst3::IPlugFrame {
public:
    vst3::tresult STDMETHODCALLTYPE queryInterface(const vst3::TUID* iid, void** obj) override {
        if (vst3::tuid_eq(iid, vst3::IID_FUnknown) || vst3::tuid_eq(iid, vst3::IID_IPlugFrame)) {
            *obj = static_cast<vst3::IPlugFrame*>(this);
            addRef();
            return vst3::kResultOk;
        }
        *obj = nullptr;
        return vst3::kNoInterface;
    }
    vst3::uint32 STDMETHODCALLTYPE addRef() override { return 1; }
    vst3::uint32 STDMETHODCALLTYPE release() override { return 1; }
    vst3::tresult STDMETHODCALLTYPE resizeView(vst3::IPlugView* view, vst3::ViewRect* newSize) override {
        if (!newSize || !g_hwnd) return vst3::kInvalidArgument;
        RECT wr{0, 0, newSize->right - newSize->left, newSize->bottom - newSize->top};
        AdjustWindowRectEx(&wr, static_cast<DWORD>(GetWindowLongW(g_hwnd, GWL_STYLE)), FALSE,
                           static_cast<DWORD>(GetWindowLongW(g_hwnd, GWL_EXSTYLE)));
        SetWindowPos(g_hwnd, nullptr, 0, 0, wr.right - wr.left, wr.bottom - wr.top,
                     SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        if (view) view->onSize(newSize);
        return vst3::kResultOk;
    }
};

static LRESULT CALLBACK vst_wnd_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    switch (msg) {
        case WM_CLOSE:
            g_gui_open.store(false);
            ShowWindow(hwnd, SW_HIDE);
            return 0;
        case WM_SIZE:
            if (g_view && wp != SIZE_MINIMIZED) {
                vst3::ViewRect rc{0, 0, LOWORD(lp), HIWORD(lp)};
                g_view->onSize(&rc);
            }
            return 0;
        case WM_SETFOCUS:
            if (g_view) g_view->onFocus(1);
            return 0;
        case WM_KILLFOCUS:
            if (g_view) g_view->onFocus(0);
            return 0;
        default:
            return DefWindowProcW(hwnd, msg, wp, lp);
    }
}

static int connect_unix_socket(const char* path) {
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) return -1;
    sockaddr_un addr{};
    addr.sun_family = AF_UNIX;
    if (std::strlen(path) >= sizeof(addr.sun_path)) {
        close(fd);
        return -1;
    }
    std::strcpy(addr.sun_path, path);
    if (connect(fd, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) != 0) {
        close(fd);
        return -1;
    }
    return fd;
}

static bool write_all(int fd, const void* buf, size_t len) {
    const uint8_t* p = static_cast<const uint8_t*>(buf);
    size_t sent = 0;
    while (sent < len) {
        ssize_t n = write(fd, p + sent, len - sent);
        if (n <= 0) return false;
        sent += static_cast<size_t>(n);
    }
    return true;
}

static bool read_exact(int fd, void* buf, size_t len) {
    uint8_t* p = static_cast<uint8_t*>(buf);
    size_t got = 0;
    while (got < len) {
        ssize_t n = read(fd, p + got, len - got);
        if (n <= 0) return false;
        got += static_cast<size_t>(n);
    }
    return true;
}

struct AudioThreadArgs {
    int audio_fd;
    ShmAudioBlock* shm;
    vst3::IAudioProcessor* processor;
    int32_t in_channels;
    int32_t out_channels;
    std::atomic<bool>* running;
};

static DWORD WINAPI audio_thread_entry(LPVOID raw) {
    auto* args = static_cast<AudioThreadArgs*>(raw);
    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL);

    HostParamChanges param_changes;
    HostParamChanges out_param_changes;
    HostEventList in_events;
    HostEventList out_events;
    std::vector<float> mono_in(SHM_MAX_FRAMES, 0.0f);

    float* in_ptrs[2] = {args->shm->in_l, args->shm->in_r};
    float* out_ptrs[2] = {args->shm->out_l, args->shm->out_r};

    vst3::AudioBusBuffers in_bus{};
    vst3::AudioBusBuffers out_bus{};

    vst3::ProcessContext ctx{};
    ctx.state = (1u << 1) | (1u << 8) | (1u << 9) | (1u << 10) | (1u << 13) | (1u << 17);
    ctx.sampleRate = args->shm->sample_rate;
    ctx.tempo = 120.0;
    ctx.timeSigNumerator = 4;
    ctx.timeSigDenominator = 4;

    vst3::ProcessData data{};
    data.processMode = vst3::kRealtime;
    data.symbolicSampleSize = vst3::kSample32;
    data.numInputs = args->in_channels > 0 ? 1 : 0;
    data.numOutputs = args->out_channels > 0 ? 1 : 0;
    data.inputs = data.numInputs ? &in_bus : nullptr;
    data.outputs = data.numOutputs ? &out_bus : nullptr;
    data.inputParameterChanges = &param_changes;
    data.outputParameterChanges = &out_param_changes;
    data.inputEvents = &in_events;
    data.outputEvents = &out_events;
    data.processContext = &ctx;

    while (args->running->load()) {
        uint8_t cmd = 0;
        if (!read_exact(args->audio_fd, &cmd, 1)) break;
        if (cmd == 'Q') break;
        if (cmd != 'P') continue;

        uint32_t frames = std::min<uint32_t>(args->shm->num_samples, SHM_MAX_FRAMES);
        double sr = args->shm->sample_rate > 0.0 ? args->shm->sample_rate : 48000.0;
        ctx.sampleRate = sr;
        data.numSamples = static_cast<int32_t>(frames);

        if (args->in_channels == 1) {
            for (uint32_t i = 0; i < frames; ++i) {
                mono_in[i] = (args->shm->in_l[i] + args->shm->in_r[i]) * 0.5f;
            }
            in_ptrs[0] = mono_in.data();
            in_ptrs[1] = mono_in.data();
        } else {
            in_ptrs[0] = args->shm->in_l;
            in_ptrs[1] = args->shm->in_r;
        }
        out_ptrs[0] = args->shm->out_l;
        out_ptrs[1] = args->shm->out_r;

        std::memset(args->shm->out_l, 0, frames * sizeof(float));
        std::memset(args->shm->out_r, 0, frames * sizeof(float));

        in_bus.numChannels = args->in_channels;
        in_bus.silenceFlags = 0;
        in_bus.channelBuffers32 = in_ptrs;

        out_bus.numChannels = args->out_channels;
        out_bus.silenceFlags = 0;
        out_bus.channelBuffers32 = out_ptrs;

        param_changes.clear();
        out_param_changes.clear();
        uint32_t pcount = std::min<uint32_t>(args->shm->pending_param_count, SHM_MAX_PARAMS);
        for (uint32_t i = 0; i < pcount; ++i) {
            param_changes.push(args->shm->pending_param_ids[i], args->shm->pending_param_vals[i]);
        }
        args->shm->pending_param_count = 0;

        {
            std::lock_guard<std::mutex> lock(g_edit_mutex);
            for (const auto& edit : g_gui_edits) {
                param_changes.push(edit.id, edit.value);
            }
            g_gui_edits.clear();
        }

        vst3::tresult res = args->processor->process(&data);
        ctx.projectTimeSamples += frames;
        ctx.continousTimeSamples += frames;
        ctx.projectTimeMusic += (static_cast<double>(frames) / sr) * (ctx.tempo / 60.0);

        uint8_t ack = (res >= 0) ? 1 : 0;
        if (!write_all(args->audio_fd, &ack, 1)) break;
    }
    return 0;
}

static std::wstring unix_to_win_wide(const std::string& path) {
    std::string dos = path;
    if (!dos.empty() && dos[0] == '/') {
        dos = "Z:" + dos;
    }
    for (char& c : dos) {
        if (c == '/') c = '\\';
    }
    int len = MultiByteToWideChar(CP_UTF8, 0, dos.c_str(), -1, nullptr, 0);
    if (len <= 0) return {};
    std::wstring out(static_cast<size_t>(len), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, dos.c_str(), -1, out.data(), len);
    if (!out.empty() && out.back() == L'\0') out.pop_back();
    return out;
}

static std::string build_params_json(vst3::IEditController* ctrl) {
    if (!ctrl) return "[]";
    vst3::int32 count = ctrl->getParameterCount();
    std::ostringstream os;
    os << "[";
    bool first = true;
    for (vst3::int32 i = 0; i < count; ++i) {
        vst3::ParameterInfo info{};
        if (ctrl->getParameterInfo(i, &info) != vst3::kResultOk) continue;
        double val = ctrl->getParamNormalized(info.id);
        vst3::String128 str{};
        std::string formatted;
        if (ctrl->getParamStringByValue(info.id, val, &str) == vst3::kResultOk) {
            formatted = utf16_to_utf8(str);
        }
        if (formatted.empty()) {
            char buf[32];
            std::snprintf(buf, sizeof(buf), "%.2f", val);
            formatted = buf;
        }
        if (!first) os << ",";
        first = false;
        os << "{"
           << "\"id\":" << info.id << ","
           << "\"name\":\"" << json_escape(utf16_to_utf8(info.title)) << "\","
           << "\"value\":" << val << ","
           << "\"default\":" << info.defaultNormalizedValue << ","
           << "\"unit\":\"" << json_escape(utf16_to_utf8(info.units)) << "\","
           << "\"step_count\":" << info.stepCount << ","
           << "\"can_automate\":" << ((info.flags & 1) ? "true" : "false") << ","
           << "\"is_read_only\":" << ((info.flags & 2) ? "true" : "false") << ","
           << "\"is_bypass\":" << ((info.flags & 65536) ? "true" : "false") << ","
           << "\"flags\":" << info.flags << ","
           << "\"formatted\":\"" << json_escape(formatted) << "\""
           << "}";
    }
    os << "]";
    return os.str();
}

int WINAPI WinMain(HINSTANCE hInst, HINSTANCE, LPSTR, int) {
    OleInitialize(nullptr);
    SetProcessDPIAware();

    const char* dll_env = std::getenv("CHRONOMIX_DLL");
    const char* shm_env = std::getenv("CHRONOMIX_SHM");
    const char* ctrl_env = std::getenv("CHRONOMIX_CTRL_SOCK");
    const char* audio_env = std::getenv("CHRONOMIX_AUDIO_SOCK");
    const char* sr_env = std::getenv("CHRONOMIX_SR");
    const char* block_env = std::getenv("CHRONOMIX_BLOCK");

    if (!dll_env || !shm_env || !ctrl_env || !audio_env) {
        return 2;
    }

    double sample_rate = sr_env ? std::atof(sr_env) : 48000.0;
    if (sample_rate <= 0.0) sample_rate = 48000.0;
    int32_t block_size = block_env ? std::atoi(block_env) : 512;
    if (block_size <= 0 || block_size > static_cast<int32_t>(SHM_MAX_FRAMES)) block_size = 512;

    int ctrl_fd = connect_unix_socket(ctrl_env);
    int audio_fd = connect_unix_socket(audio_env);
    if (ctrl_fd < 0 || audio_fd < 0) return 3;

    auto send_err = [&](const std::string& msg) {
        std::string line = "{\"error\":\"" + json_escape(msg) + "\"}\n";
        write_all(ctrl_fd, line.data(), line.size());
    };

    int shm_fd = open(shm_env, O_RDWR);
    if (shm_fd < 0) {
        send_err("Failed to open shared memory file");
        return 4;
    }
    auto* shm = static_cast<ShmAudioBlock*>(
        mmap(nullptr, sizeof(ShmAudioBlock), PROT_READ | PROT_WRITE, MAP_SHARED, shm_fd, 0));
    close(shm_fd);
    if (shm == MAP_FAILED || shm->magic != SHM_MAGIC) {
        send_err("Invalid shared memory mapping");
        return 5;
    }

    std::wstring dll_w = unix_to_win_wide(dll_env);
    auto slash_pos = dll_w.find_last_of(L'\\');
    if (slash_pos != std::wstring::npos) {
        std::wstring dir = dll_w.substr(0, slash_pos);
        SetDllDirectoryW(dir.c_str());
    }

    HMODULE mod = LoadLibraryW(dll_w.c_str());
    if (!mod) {
        DWORD err = GetLastError();
        send_err("LoadLibraryW failed with Win32 error " + std::to_string(err));
        return 6;
    }

    using InitDllFn = bool(STDMETHODCALLTYPE*)();
    using ExitDllFn = bool(STDMETHODCALLTYPE*)();
    using GetFactoryFn = vst3::IPluginFactory*(STDMETHODCALLTYPE*)();

    if (auto init_fn = reinterpret_cast<InitDllFn>(reinterpret_cast<void*>(GetProcAddress(mod, "InitDll")))) {
        init_fn();
    }
    auto get_factory = reinterpret_cast<GetFactoryFn>(reinterpret_cast<void*>(GetProcAddress(mod, "GetPluginFactory")));
    if (!get_factory) {
        send_err("GetPluginFactory symbol not found in Windows VST3 DLL");
        FreeLibrary(mod);
        return 7;
    }

    vst3::IPluginFactory* factory = get_factory();
    if (!factory) {
        send_err("GetPluginFactory returned null");
        FreeLibrary(mod);
        return 8;
    }

    vst3::PFactoryInfo finfo{};
    factory->getFactoryInfo(&finfo);

    vst3::IPluginFactory2* factory2 = nullptr;
    factory->queryInterface(&vst3::IID_IPluginFactory2, reinterpret_cast<void**>(&factory2));

    vst3::TUID plugin_cid{};
    std::string plugin_name = "Windows VST3";
    std::string plugin_vendor = finfo.vendor;
    std::string plugin_version = "1.0";
    std::string plugin_category = "Fx";
    bool found_class = false;

    int32_t class_count = factory->countClasses();
    for (int32_t i = 0; i < class_count; ++i) {
        vst3::PClassInfo ci{};
        if (factory->getClassInfo(i, &ci) != vst3::kResultOk) continue;
        if (std::strcmp(ci.category, "Audio Module Class") == 0) {
            plugin_cid = ci.cid;
            plugin_name = ci.name;
            found_class = true;
            if (factory2) {
                vst3::PClassInfo2 ci2{};
                if (factory2->getClassInfo2(i, &ci2) == vst3::kResultOk) {
                    if (ci2.vendor[0]) plugin_vendor = ci2.vendor;
                    if (ci2.version[0]) plugin_version = ci2.version;
                    if (ci2.subCategories[0]) plugin_category = ci2.subCategories;
                }
            }
            break;
        }
    }
    if (factory2) factory2->release();

    if (!found_class) {
        send_err("No Audio Module Class found inside VST3");
        factory->release();
        FreeLibrary(mod);
        return 9;
    }

    vst3::IComponent* component = nullptr;
    if (factory->createInstance(reinterpret_cast<const char*>(&plugin_cid),
                                reinterpret_cast<const char*>(&vst3::IID_IComponent),
                                reinterpret_cast<void**>(&component)) != vst3::kResultOk ||
        !component) {
        send_err("Failed to create IComponent instance");
        factory->release();
        FreeLibrary(mod);
        return 10;
    }

    HostApplication host_app;
    if (component->initialize(&host_app) != vst3::kResultOk) {
        send_err("IComponent::initialize failed");
        component->release();
        factory->release();
        FreeLibrary(mod);
        return 11;
    }

    vst3::IAudioProcessor* processor = nullptr;
    if (component->queryInterface(&vst3::IID_IAudioProcessor, reinterpret_cast<void**>(&processor)) != vst3::kResultOk ||
        !processor) {
        send_err("Plugin does not implement IAudioProcessor");
        component->terminate();
        component->release();
        factory->release();
        FreeLibrary(mod);
        return 12;
    }

    vst3::IEditController* controller = nullptr;
    bool separate_controller = false;
    if (component->queryInterface(&vst3::IID_IEditController, reinterpret_cast<void**>(&controller)) != vst3::kResultOk ||
        !controller) {
        vst3::TUID ctrl_cid{};
        if (component->getControllerClassId(&ctrl_cid) == vst3::kResultOk) {
            if (factory->createInstance(reinterpret_cast<const char*>(&ctrl_cid),
                                        reinterpret_cast<const char*>(&vst3::IID_IEditController),
                                        reinterpret_cast<void**>(&controller)) == vst3::kResultOk &&
                controller) {
                separate_controller = true;
                controller->initialize(&host_app);
            }
        }
    }

    ComponentHandler handler;
    if (controller) {
        controller->setComponentHandler(&handler);
    }

    vst3::IConnectionPoint* comp_cp = nullptr;
    vst3::IConnectionPoint* ctrl_cp = nullptr;
    if (separate_controller && controller &&
        component->queryInterface(&vst3::IID_IConnectionPoint, reinterpret_cast<void**>(&comp_cp)) == vst3::kResultOk &&
        controller->queryInterface(&vst3::IID_IConnectionPoint, reinterpret_cast<void**>(&ctrl_cp)) == vst3::kResultOk &&
        comp_cp && ctrl_cp) {
        comp_cp->connect(ctrl_cp);
        ctrl_cp->connect(comp_cp);
    }

    int32_t in_ch = 2;
    int32_t out_ch = 2;
    if (component->getBusCount(vst3::kMediaAudio, vst3::kDirInput) > 0) {
        vst3::BusInfo bi{};
        if (component->getBusInfo(vst3::kMediaAudio, vst3::kDirInput, 0, &bi) == vst3::kResultOk) {
            in_ch = std::clamp<int32_t>(bi.channelCount, 1, 2);
        }
    } else {
        in_ch = 0;
    }
    if (component->getBusCount(vst3::kMediaAudio, vst3::kDirOutput) > 0) {
        vst3::BusInfo bi{};
        if (component->getBusInfo(vst3::kMediaAudio, vst3::kDirOutput, 0, &bi) == vst3::kResultOk) {
            out_ch = std::clamp<int32_t>(bi.channelCount, 1, 2);
        }
    } else {
        out_ch = 0;
    }

    if (in_ch > 0 && out_ch > 0) {
        vst3::SpeakerArrangement stereo = 0x3ULL;
        vst3::SpeakerArrangement mono = 0x1ULL;
        if (processor->setBusArrangements(&stereo, 1, &stereo, 1) == vst3::kResultOk) {
            in_ch = 2;
            out_ch = 2;
        } else if (processor->setBusArrangements(&mono, 1, &stereo, 1) == vst3::kResultOk) {
            in_ch = 1;
            out_ch = 2;
        } else {
            vst3::SpeakerArrangement in_arr = (in_ch == 1) ? mono : stereo;
            vst3::SpeakerArrangement out_arr = (out_ch == 1) ? mono : stereo;
            processor->setBusArrangements(&in_arr, 1, &out_arr, 1);
        }
    }

    vst3::ProcessSetup setup{vst3::kRealtime, vst3::kSample32, static_cast<int32_t>(SHM_MAX_FRAMES), sample_rate};
    processor->setupProcessing(&setup);

    if (in_ch > 0) component->activateBus(vst3::kMediaAudio, vst3::kDirInput, 0, 1);
    if (out_ch > 0) component->activateBus(vst3::kMediaAudio, vst3::kDirOutput, 0, 1);

    if (separate_controller && controller) {
        HostMemoryStream init_state;
        if (component->getState(&init_state) == vst3::kResultOk && !init_state.data.empty()) {
            init_state.cursor = 0;
            controller->setComponentState(&init_state);
        }
    }

    component->setActive(1);
    processor->setProcessing(1);

    bool has_gui = (controller != nullptr);

    {
        std::ostringstream os;
        os << "{"
           << "\"name\":\"" << json_escape(plugin_name) << "\","
           << "\"vendor\":\"" << json_escape(plugin_vendor) << "\","
           << "\"version\":\"" << json_escape(plugin_version) << "\","
           << "\"category\":\"" << json_escape(plugin_category) << "\","
           << "\"in_channels\":" << in_ch << ","
           << "\"out_channels\":" << out_ch << ","
           << "\"has_gui\":" << (has_gui ? "true" : "false") << ","
           << "\"params\":" << build_params_json(controller)
           << "}\n";
        std::string init_json = os.str();
        write_all(ctrl_fd, init_json.data(), init_json.size());
    }

    std::atomic<bool> running{true};
    AudioThreadArgs audio_args{audio_fd, shm, processor, in_ch, out_ch, &running};
    HANDLE audio_thread = CreateThread(nullptr, 0, audio_thread_entry, &audio_args, 0, nullptr);

    auto write_str = [](int fd, const std::string& s) {
        return write_all(fd, s.data(), s.size());
    };

    HINSTANCE wnd_hinst = hInst ? hInst : GetModuleHandleW(nullptr);
    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.style = CS_HREDRAW | CS_VREDRAW | CS_OWNDC;
    wc.lpfnWndProc = vst_wnd_proc;
    wc.hInstance = wnd_hinst;
    wc.hCursor = LoadCursorW(nullptr, MAKEINTRESOURCEW(32512));
    wc.hbrBackground = reinterpret_cast<HBRUSH>(COLOR_WINDOW + 1);
    wc.lpszClassName = L"ChronomixWineVstWnd";
    ATOM wnd_cls = RegisterClassExW(&wc);
    DWORD wnd_reg_err = (wnd_cls == 0) ? GetLastError() : 0;

    PlugFrame plug_frame;
    std::string recv_buf;
    char tmp[4096];

    while (running.load()) {
        MSG msg;
        while (PeekMessageW(&msg, nullptr, 0, 0, PM_REMOVE)) {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        pollfd pfd{ctrl_fd, POLLIN, 0};
        if (poll(&pfd, 1, 8) > 0 && (pfd.revents & POLLIN)) {
            ssize_t n = read(ctrl_fd, tmp, sizeof(tmp));
            if (n <= 0) {
                running.store(false);
                break;
            }
            recv_buf.append(tmp, static_cast<size_t>(n));
            size_t nl = 0;
            while ((nl = recv_buf.find('\n')) != std::string::npos) {
                std::string cmd = recv_buf.substr(0, nl);
                recv_buf.erase(0, nl + 1);

                if (cmd == "QUIT") {
                    running.store(false);
                    break;
                } else if (cmd == "GET_PARAMS") {
                    std::string reply = build_params_json(controller) + "\n";
                    write_all(ctrl_fd, reply.data(), reply.size());
                } else if (cmd.rfind("SET_PARAM ", 0) == 0) {
                    uint32_t id = 0;
                    double val = 0.0;
                    if (std::sscanf(cmd.c_str() + 10, "%u %lf", &id, &val) == 2 && controller) {
                        controller->setParamNormalized(id, val);
                        vst3::String128 str{};
                        std::string formatted;
                        if (controller->getParamStringByValue(id, val, &str) == vst3::kResultOk) {
                            formatted = utf16_to_utf8(str);
                        }
                        if (formatted.empty()) {
                            char fbuf[32];
                            std::snprintf(fbuf, sizeof(fbuf), "%.2f", val);
                            formatted = fbuf;
                        }
                        std::string reply = formatted + "\n";
                        write_all(ctrl_fd, reply.data(), reply.size());
                    } else {
                        write_all(ctrl_fd, "\n", 1);
                    }
                } else if (cmd == "OPEN_GUI") {
                    if (g_hwnd && g_view) {
                        ShowWindow(g_hwnd, SW_SHOWNORMAL);
                        UpdateWindow(g_hwnd);
                        SetForegroundWindow(g_hwnd);
                        g_gui_open.store(true);
                        write_str(ctrl_fd, "OK\n");
                    } else if (wnd_cls == 0) {
                        write_str(ctrl_fd, "ERR: RegisterClassExW failed (" + std::to_string(wnd_reg_err) + ")\n");
                    } else if (controller) {
                        g_view = controller->createView("editor");
                        if (!g_view) {
                            write_str(ctrl_fd, "ERR: createView(\"editor\") returned null\n");
                        } else {
                            vst3::tresult plat_res = g_view->isPlatformTypeSupported("HWND");
                            if (plat_res != vst3::kResultOk) {
                                g_view->release();
                                g_view = nullptr;
                                write_str(ctrl_fd, "ERR: isPlatformTypeSupported(\"HWND\") returned " +
                                    std::to_string(plat_res) + "\n");
                            } else {
                                vst3::ViewRect rect{0, 0, 800, 500};
                                g_view->getSize(&rect);
                                int w = std::max(200, rect.right - rect.left);
                                int h = std::max(150, rect.bottom - rect.top);
                                DWORD style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
                                RECT wr{0, 0, w, h};
                                AdjustWindowRectEx(&wr, style, FALSE, 0);
                                std::wstring title = L"Chronomix [Wine] - " +
                                    std::wstring(plugin_name.begin(), plugin_name.end());
                                g_hwnd = CreateWindowExW(
                                    0, wc.lpszClassName, title.c_str(), style,
                                    CW_USEDEFAULT, CW_USEDEFAULT, wr.right - wr.left, wr.bottom - wr.top,
                                    nullptr, nullptr, wnd_hinst, nullptr);
                                if (!g_hwnd) {
                                    DWORD err = GetLastError();
                                    g_view->release();
                                    g_view = nullptr;
                                    std::string hint = (err == 1400) ? " - no Wine display driver / DISPLAY" : "";
                                    write_str(ctrl_fd, "ERR: CreateWindowExW failed (" +
                                        std::to_string(err) + hint + ")\n");
                                } else {
                                    g_view->setFrame(&plug_frame);
                                    vst3::tresult att_res = g_view->attached(g_hwnd, "HWND");
                                    if (att_res == vst3::kResultOk) {
                                        vst3::ViewRect post_rect{0, 0, w, h};
                                        if (g_view->getSize(&post_rect) == vst3::kResultOk) {
                                            int pw = std::max(200, post_rect.right - post_rect.left);
                                            int ph = std::max(150, post_rect.bottom - post_rect.top);
                                            if (pw != w || ph != h) {
                                                RECT pwr{0, 0, pw, ph};
                                                AdjustWindowRectEx(&pwr, style, FALSE, 0);
                                                SetWindowPos(g_hwnd, nullptr, 0, 0, pwr.right - pwr.left, pwr.bottom - pwr.top,
                                                             SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
                                            }
                                        }
                                        ShowWindow(g_hwnd, SW_SHOWNORMAL);
                                        UpdateWindow(g_hwnd);
                                        SetForegroundWindow(g_hwnd);
                                        MSG m;
                                        while (PeekMessageW(&m, nullptr, 0, 0, PM_REMOVE)) {
                                            TranslateMessage(&m);
                                            DispatchMessageW(&m);
                                        }
                                        g_gui_open.store(true);
                                        write_str(ctrl_fd, "OK\n");
                                    } else {
                                        DestroyWindow(g_hwnd);
                                        g_hwnd = nullptr;
                                        g_view->setFrame(nullptr);
                                        g_view->release();
                                        g_view = nullptr;
                                        write_str(ctrl_fd, "ERR: IPlugView::attached(HWND) failed (" +
                                            std::to_string(att_res) + ")\n");
                                    }
                                }
                            }
                        }
                    } else {
                        write_str(ctrl_fd, "ERR: Plugin has no IEditController\n");
                    }
                } else if (cmd == "CLOSE_GUI") {
                    g_gui_open.store(false);
                    if (g_hwnd) ShowWindow(g_hwnd, SW_HIDE);
                    write_str(ctrl_fd, "OK\n");
                } else if (cmd == "IS_GUI_OPEN") {
                    write_str(ctrl_fd, g_gui_open.load() ? "1\n" : "0\n");
                }
            }
        } else if (pfd.revents & (POLLHUP | POLLERR)) {
            running.store(false);
            break;
        }
    }

    running.store(false);
    shutdown(audio_fd, SHUT_RDWR);
    if (audio_thread) {
        WaitForSingleObject(audio_thread, 2000);
        CloseHandle(audio_thread);
    }

    if (g_view) {
        g_view->removed();
        g_view->setFrame(nullptr);
        g_view->release();
        g_view = nullptr;
    }
    if (g_hwnd) {
        DestroyWindow(g_hwnd);
        g_hwnd = nullptr;
    }

    processor->setProcessing(0);
    component->setActive(0);
    if (comp_cp && ctrl_cp) {
        comp_cp->disconnect(ctrl_cp);
        ctrl_cp->disconnect(comp_cp);
    }
    if (comp_cp) comp_cp->release();
    if (ctrl_cp) ctrl_cp->release();
    if (controller) {
        controller->setComponentHandler(nullptr);
        if (separate_controller) controller->terminate();
        controller->release();
    }
    processor->release();
    component->terminate();
    component->release();
    factory->release();
    if (auto exit_fn = reinterpret_cast<ExitDllFn>(reinterpret_cast<void*>(GetProcAddress(mod, "ExitDll")))) {
        exit_fn();
    }
    FreeLibrary(mod);
    munmap(shm, sizeof(ShmAudioBlock));
    close(audio_fd);
    close(ctrl_fd);
    OleUninitialize();
    return 0;
}
