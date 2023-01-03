#ifndef LATENCYFLEX2_H
#define LATENCYFLEX2_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#ifdef _WIN32
#define LFX2_API __declspec(dllimport)
#else
#define LFX2_API
#endif


typedef enum lfx2MarkType {
  lfx2MarkTypeBegin,
  lfx2MarkTypeEnd,
} lfx2MarkType;

typedef struct lfx2Context lfx2Context;

/**
 * A write handle for frame markers.
 */
typedef struct lfx2Frame lfx2Frame;

typedef struct lfx2ImplicitContext lfx2ImplicitContext;

typedef uint64_t lfx2Timestamp;

typedef uint32_t lfx2SectionId;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

LFX2_API lfx2Timestamp lfx2TimestampNow(void);

#if defined(_WIN32)
LFX2_API lfx2Timestamp lfx2TimestampFromQpc(uint64_t qpc);
#endif

LFX2_API void lfx2SleepUntil(lfx2Timestamp target);

LFX2_API struct lfx2Context *lfx2ContextCreate(void);

LFX2_API void lfx2ContextAddRef(struct lfx2Context *context);

LFX2_API void lfx2ContextRelease(struct lfx2Context *context);

LFX2_API
struct lfx2Frame *lfx2FrameCreate(struct lfx2Context *context,
                                  lfx2Timestamp *out_timestamp);

LFX2_API void lfx2FrameAddRef(struct lfx2Frame *frame);

LFX2_API void lfx2FrameRelease(struct lfx2Frame *frame);

LFX2_API
void lfx2MarkSection(struct lfx2Frame *frame,
                     lfx2SectionId section_id,
                     enum lfx2MarkType mark_type,
                     lfx2Timestamp timestamp);

LFX2_API struct lfx2ImplicitContext *lfx2ImplicitContextCreate(void);

LFX2_API void lfx2ImplicitContextRelease(struct lfx2ImplicitContext *context);

LFX2_API void lfx2ImplicitContextReset(struct lfx2ImplicitContext *context);

LFX2_API
struct lfx2Frame *lfx2FrameCreateImplicit(struct lfx2ImplicitContext *context,
                                          lfx2Timestamp *out_timestamp);

LFX2_API
struct lfx2Frame *lfx2FrameDequeueImplicit(struct lfx2ImplicitContext *context,
                                           bool critical);

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus

#endif /* LATENCYFLEX2_H */
