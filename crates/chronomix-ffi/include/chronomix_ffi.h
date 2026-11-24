#pragma once

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ChronomixAppHandle ChronomixAppHandle;

typedef struct CChannelMeter {
    float in_left;
    float in_right;
    float out_left;
    float out_right;
    float comp_gr_db;
    uint64_t media_pos_frames;
    uint8_t media_playing;
    uint8_t _pad[7];
} CChannelMeter;

typedef struct CMixerTelemetry {
    uint32_t channel_count;
    float master_left;
    float master_right;
    float virtual_mic_left;
    float virtual_mic_right;
    uint8_t is_virtual_mic_active;
    uint8_t is_recording;
    uint8_t is_audio_running;
    uint8_t _pad0;
    uint64_t recorded_frames;
    uint64_t underruns;
    uint64_t overruns;
    uint32_t sample_rate;
    uint32_t block_size;
    CChannelMeter channels[48];
} CMixerTelemetry;

typedef struct CDjDeckParams {
    float gain_db;
    float volume;
    float eq_low_db;
    float eq_mid_db;
    float eq_high_db;
    uint8_t kill_low;
    uint8_t kill_mid;
    uint8_t kill_high;
    uint8_t pfl_cue;
    float filter;
    float tempo_percent;
    uint8_t loop_enabled;
    uint8_t _pad[3];
    float loop_beats;
} CDjDeckParams;

typedef struct CDjMixerParams {
    float crossfader;
    uint8_t curve;
    uint8_t send_to_master;
    uint8_t _pad[2];
    float master_volume;
    float cue_mix;
} CDjMixerParams;

typedef struct CDjDeckTelemetry {
    uint64_t pos_frames;
    uint64_t total_frames;
    uint32_t sample_rate;
    float base_bpm;
    float effective_bpm;
    float peak_l;
    float peak_r;
    double cue_frame;
    double loop_start_frame;
    double loop_end_frame;
    double hot_cues[4];
    uint8_t hot_cue_set[4];
    uint8_t playing;
    uint8_t loop_enabled;
    uint8_t has_track;
    uint8_t _pad;
} CDjDeckTelemetry;

typedef struct CDjTelemetry {
    CDjDeckTelemetry decks[2];
    float master_peak_l;
    float master_peak_r;
    float crossfader;
} CDjTelemetry;

typedef struct CWaveformBin {
    float low;
    float mid;
    float high;
    float peak;
} CWaveformBin;

ChronomixAppHandle* chronomix_create(void);
void chronomix_destroy(ChronomixAppHandle* handle);
void chronomix_free_string(char* ptr);
void chronomix_tick(ChronomixAppHandle* handle);

char* chronomix_get_paths_json(const ChronomixAppHandle* handle);
char* chronomix_get_settings_json(const ChronomixAppHandle* handle);
int32_t chronomix_update_settings_json(ChronomixAppHandle* handle, const char* json_str);
char* chronomix_take_last_error(ChronomixAppHandle* handle);
char* chronomix_get_status_text(const ChronomixAppHandle* handle);
char* chronomix_get_wine_warning(const ChronomixAppHandle* handle);

char* chronomix_list_devices_json(const ChronomixAppHandle* handle);
void chronomix_refresh_devices(ChronomixAppHandle* handle);
int32_t chronomix_set_output_device(ChronomixAppHandle* handle, const char* output_device_id, uint32_t sample_rate, uint32_t block_size);
int32_t chronomix_start_audio(ChronomixAppHandle* handle, const char* output_device_id, uint32_t sample_rate, uint32_t block_size);
void chronomix_stop_audio(ChronomixAppHandle* handle);
int32_t chronomix_toggle_virtual_mic(ChronomixAppHandle* handle, uint8_t enable);
void chronomix_set_virtual_mic_gain_db(ChronomixAppHandle* handle, float gain_db);
int32_t chronomix_start_recording(ChronomixAppHandle* handle, const char* base_dir);
char* chronomix_stop_recording(ChronomixAppHandle* handle);

void chronomix_get_mixer_telemetry(ChronomixAppHandle* handle, CMixerTelemetry* out_tel);
char* chronomix_get_mixer_state_json(const ChronomixAppHandle* handle);
int32_t chronomix_add_channel(ChronomixAppHandle* handle);
int32_t chronomix_add_multimedia_channel(ChronomixAppHandle* handle);
int32_t chronomix_remove_channel(ChronomixAppHandle* handle, uint32_t ch);
void chronomix_set_channel_name(ChronomixAppHandle* handle, uint32_t ch, const char* name);
void chronomix_set_channel_color(ChronomixAppHandle* handle, uint32_t ch, uint8_t r, uint8_t g, uint8_t b);
int32_t chronomix_set_channel_dsp_json(ChronomixAppHandle* handle, uint32_t ch, const char* dsp_json);
char* chronomix_get_channel_dsp_json(const ChronomixAppHandle* handle, uint32_t ch);
float chronomix_eval_channel_eq_db(const ChronomixAppHandle* handle, uint32_t ch, float freq_hz);
int32_t chronomix_set_channel_input(ChronomixAppHandle* handle, uint32_t ch, const char* device_id, const char* device_label, uint8_t mode);
void chronomix_set_channel_multimedia(ChronomixAppHandle* handle, uint32_t ch);
int32_t chronomix_load_channel_media_pcm(ChronomixAppHandle* handle, uint32_t ch, const float* interleaved_stereo, uint32_t total_frames, uint32_t sample_rate, const char* title, const char* path);
int32_t chronomix_load_channel_media_file(ChronomixAppHandle* handle, uint32_t ch, const char* path);
void chronomix_unload_channel_media(ChronomixAppHandle* handle, uint32_t ch);
void chronomix_set_media_transport(ChronomixAppHandle* handle, uint32_t ch, uint8_t playing, uint8_t looping, int64_t seek_frame);
void chronomix_set_all_media_transport(ChronomixAppHandle* handle, uint8_t playing);
void chronomix_clear_all_solos(ChronomixAppHandle* handle);
void chronomix_set_master(ChronomixAppHandle* handle, float fader_db, uint8_t mute, uint8_t dim, uint8_t record_mix);

char* chronomix_get_presets_json(void);
int32_t chronomix_apply_preset(ChronomixAppHandle* handle, uint32_t ch, uint32_t preset_index);
int32_t chronomix_save_channel_profile_file(const ChronomixAppHandle* handle, uint32_t ch, const char* path);
int32_t chronomix_load_channel_profile_file(ChronomixAppHandle* handle, uint32_t ch, const char* path);

char* chronomix_scan_plugins_json(ChronomixAppHandle* handle);
int32_t chronomix_load_insert_plugin(ChronomixAppHandle* handle, uint8_t is_master, uint32_t ch, uint32_t slot, const char* path);
void chronomix_unload_insert_plugin(ChronomixAppHandle* handle, uint8_t is_master, uint32_t ch, uint32_t slot);
void chronomix_set_insert_bypass_mix(ChronomixAppHandle* handle, uint8_t is_master, uint32_t ch, uint32_t slot, uint8_t bypass, float mix);
char* chronomix_set_insert_param(ChronomixAppHandle* handle, uint8_t is_master, uint32_t ch, uint32_t slot, uint32_t param_id, double value);
void chronomix_refresh_insert_params(ChronomixAppHandle* handle, uint8_t is_master, uint32_t ch, uint32_t slot);
int32_t chronomix_open_insert_gui(ChronomixAppHandle* handle, uint8_t is_master, uint32_t ch, uint32_t slot);
void chronomix_close_insert_gui(ChronomixAppHandle* handle, uint8_t is_master, uint32_t ch, uint32_t slot);

char* chronomix_midi_get_state_json(ChronomixAppHandle* handle);
int32_t chronomix_midi_add_session(ChronomixAppHandle* handle);
int32_t chronomix_midi_remove_active_session(ChronomixAppHandle* handle);
void chronomix_midi_select_session(ChronomixAppHandle* handle, uint32_t idx);
void chronomix_midi_set_transport(ChronomixAppHandle* handle, uint8_t playing, float bpm, float swing, uint8_t drums_enabled);
void chronomix_midi_set_synth(ChronomixAppHandle* handle, uint8_t preset, float attack, float release, float cutoff, float resonance, float drive, float delay_send, float delay_time_ms);
void chronomix_midi_toggle_step(ChronomixAppHandle* handle, uint32_t track_idx, uint32_t step);
void chronomix_midi_set_track_mix(ChronomixAppHandle* handle, uint32_t track_idx, float volume, float pan, uint8_t mute, uint8_t solo);
int32_t chronomix_midi_toggle_note(ChronomixAppHandle* handle, uint8_t midi_note, uint32_t step, uint32_t length_steps);
void chronomix_midi_load_demo_melody(ChronomixAppHandle* handle);
void chronomix_midi_clear_notes(ChronomixAppHandle* handle);
void chronomix_midi_trigger_key(ChronomixAppHandle* handle, uint8_t midi_note);

int32_t chronomix_dj_load_pcm(ChronomixAppHandle* handle, uint32_t deck, const float* interleaved_stereo, uint32_t total_frames, uint32_t sample_rate, const char* title, const char* path);
int32_t chronomix_dj_load_file(ChronomixAppHandle* handle, uint32_t deck, const char* path);
void chronomix_dj_unload(ChronomixAppHandle* handle, uint32_t deck);
void chronomix_dj_set_playing(ChronomixAppHandle* handle, uint32_t deck, uint8_t playing);
void chronomix_dj_seek_norm(ChronomixAppHandle* handle, uint32_t deck, double norm);
void chronomix_dj_nudge_secs(ChronomixAppHandle* handle, uint32_t deck, double delta_secs);
void chronomix_dj_set_cue(ChronomixAppHandle* handle, uint32_t deck);
void chronomix_dj_jump_cue(ChronomixAppHandle* handle, uint32_t deck, uint8_t play);
void chronomix_dj_hot_cue(ChronomixAppHandle* handle, uint32_t deck, uint32_t slot, uint8_t clear);
void chronomix_dj_sync_bpm(ChronomixAppHandle* handle, uint32_t deck);
void chronomix_dj_set_loop(ChronomixAppHandle* handle, uint32_t deck, uint8_t enabled, float beats);
void chronomix_dj_set_deck_params(ChronomixAppHandle* handle, uint32_t deck, const CDjDeckParams* params);
void chronomix_dj_get_deck_params(const ChronomixAppHandle* handle, uint32_t deck, CDjDeckParams* out_params);
void chronomix_dj_set_mixer_params(ChronomixAppHandle* handle, const CDjMixerParams* params);
void chronomix_dj_get_mixer_params(const ChronomixAppHandle* handle, CDjMixerParams* out_params);
void chronomix_dj_start_auto_crossfade(ChronomixAppHandle* handle, float target, float duration_secs);
void chronomix_dj_get_telemetry(ChronomixAppHandle* handle, CDjTelemetry* out_tel);
uint32_t chronomix_dj_get_waveform(const ChronomixAppHandle* handle, uint32_t deck, CWaveformBin* out_bins, uint32_t max_bins);
char* chronomix_dj_get_deck_title(const ChronomixAppHandle* handle, uint32_t deck);

char* chronomix_library_scan_json(const char* dir_path);

char* chronomix_wine_status_json(const ChronomixAppHandle* handle);
char* chronomix_wine_init_prefix(const ChronomixAppHandle* handle);
char* chronomix_wine_rebuild_bridge(const ChronomixAppHandle* handle);
char* chronomix_wine_launch_tool(const ChronomixAppHandle* handle, uint8_t action_id);
char* chronomix_wine_launch_exe(const ChronomixAppHandle* handle, const char* path);

#ifdef __cplusplus
}
#endif
