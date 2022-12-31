#ifndef LATENCYFLEX2_H
#define LATENCYFLEX2_H

#include <cstdarg>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <ostream>
#include <new>
#ifdef _WIN32
#define LFX2_API __declspec(dllimport)
#else
#define LFX2_API
#endif


enum class lfx2MarkType {
  lfx2MarkTypeBegin,
  lfx2MarkTypeEnd,
};

struct lfx2Context;

/// A write handle for frame markers.
struct lfx2Frame;

using lfx2Timestamp = uint64_t;

using lfx2SectionId = uint32_t;


extern "C" {

LFX2_API lfx2Timestamp lfx2TimestampNow();

#if defined(_WIN32)
LFX2_API lfx2Timestamp lfx2TimestampFromQpc(uint64_t qpc);
#endif

LFX2_API void lfx2SleepUntil(lfx2Timestamp target);

LFX2_API const lfx2Context *lfx2ContextCreate();

LFX2_API void lfx2ContextAddRef(const lfx2Context *context);

LFX2_API void lfx2ContextRelease(const lfx2Context *context);

LFX2_API const lfx2Frame *lfx2FrameCreate(const lfx2Context *context, lfx2Timestamp *out_timestamp);

LFX2_API void lfx2FrameAddRef(const lfx2Frame *frame);

LFX2_API void lfx2FrameRelease(const lfx2Frame *frame);

LFX2_API
void lfx2MarkSection(const lfx2Frame *frame,
                     lfx2SectionId section_id,
                     lfx2MarkType mark_type,
                     lfx2Timestamp timestamp);

} // extern "C"

#endif // LATENCYFLEX2_H
