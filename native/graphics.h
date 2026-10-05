#pragma once
#include <stddef.h>
#include <stdint.h>
struct SpatpitVertex {
  float x, y, u, v;
  uint32_t color;
};
struct SpatpitDraw {
  uint32_t first_index, index_count, base_vertex, texture;
  float clip[4];
};
struct SpatpitOptions {
  float sharpness, saturation, contrast, split;
  uint32_t effect, comparison, paused, reshade, neural, idle_resize;
};
struct SpatpitStatus {
  uint32_t capture_active, has_frame, width, height, reshade_loaded, techniques;
  uint64_t frames;
  uint64_t frame_age_ms;
};
struct SpatpitTechnique {
  char effect[256], name[128];
  uint32_t enabled;
};
struct SpatpitUniform {
  char name[128], label[256], category[128], tooltip[512], items[1024];
  uint32_t kind, components, bounded;
  float values[4], minimum[4], maximum[4];
};
extern "C" {
void *spatpit_create(void *hwnd, const wchar_t *runtime_folder);
void spatpit_destroy(void *instance);
const char *spatpit_error(void *instance);
int spatpit_capture(void *instance, void *target, int client_only);
void spatpit_stop(void *instance);
int spatpit_texture(void *instance, uint32_t id, uint32_t width, uint32_t height,
                   const uint8_t *rgba);
void spatpit_free_texture(void *instance, uint32_t id);
int spatpit_render(void *instance, uint32_t width, uint32_t height, float scale,
                  const SpatpitVertex *vertices, uint32_t vertex_count,
                  const uint32_t *indices, uint32_t index_count,
                  const SpatpitDraw *draws, uint32_t draw_count,
                  SpatpitOptions options);
void spatpit_status(void *instance, SpatpitStatus *status);
int spatpit_screenshot(void *instance, uint8_t *rgba, uint32_t width,
                      uint32_t height);
int spatpit_load_reshade(void *instance, const wchar_t *dll, const char *config);
void spatpit_reshade_preset(void *instance, const char *path);
void spatpit_techniques(void *, void (*)(const SpatpitTechnique *, void *),
                       void *);
void spatpit_uniforms(void *, const char *,
                     void (*)(const SpatpitUniform *, void *), void *);
int spatpit_set_technique(void *, const char *, const char *, int);
int spatpit_set_uniform(void *, const char *, const char *, const float *, int);
int spatpit_save_preset(void *);
int spatpit_native_overlay(void *, int);
int spatpit_native_overlay_active(void *);
}
