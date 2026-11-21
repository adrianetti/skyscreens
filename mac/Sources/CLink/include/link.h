#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define SK_DEFAULT_PORT 7243

#define SK_EVENT_KEYFRAME_REQUEST 1

#define SK_EVENT_PONG 2

#define SK_PRIORITY_CONTROL 0

#define SK_PRIORITY_CURSOR 1

#define SK_PRIORITY_FOCUSED 2

#define SK_PRIORITY_VIDEO 3

#define SK_PRIORITY_BACKGROUND 4

#define SK_PRIORITY_REFINE 5

typedef struct SkServer SkServer;

typedef void (*SkCallback)(void *ctx, uint8_t kind, uint8_t screen, uint64_t value);

const SkServer *sk_server_start(uint16_t port);

bool sk_server_connected(const SkServer *server);

void sk_server_on_message(const SkServer *server, SkCallback cb, void *ctx);

bool sk_set_screens(const SkServer *server,
                    uint8_t count,
                    const uint8_t *ids,
                    const uint8_t *roles,
                    const uint16_t *widths,
                    const uint16_t *heights);

bool sk_send_video(const SkServer *server,
                   uint8_t screen,
                   bool key,
                   uint64_t pts_us,
                   const uint8_t *data,
                   size_t len,
                   uint8_t priority);

bool sk_send_tile(const SkServer *server,
                  uint8_t screen,
                  uint16_t x,
                  uint16_t y,
                  uint16_t w,
                  uint16_t h,
                  const uint8_t *lz4,
                  size_t len,
                  uint8_t priority);

bool sk_send_cursor(const SkServer *server, uint8_t screen, float x, float y, uint16_t shape);
