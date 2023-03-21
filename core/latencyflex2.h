#ifndef LATENCYFLEX2_H
#define LATENCYFLEX2_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#ifdef LFX2_VK
#include <vulkan/vulkan.h>
#endif

#ifdef LFX2_DX12
#include <d3d12.h>
#endif

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

#if (defined(LFX2_DX12) && defined(_WIN32))
typedef struct lfx2Dx12Context lfx2Dx12Context;
#endif

/**
 * A write handle for frame markers.
 */
typedef struct lfx2Frame lfx2Frame;

typedef struct lfx2ImplicitContext lfx2ImplicitContext;

#if defined(LFX2_VK)
typedef struct lfx2VulkanContext lfx2VulkanContext;
#endif

#if (defined(LFX2_DX12) && defined(_WIN32))
typedef struct lfx2Dx12SubmitAux {
  ID3D12GraphicsCommandList* execute_before;
  ID3D12GraphicsCommandList* execute_after;
  ID3D12Fence* signal_fence;
  uint64_t signal_fence_value;
} lfx2Dx12SubmitAux;
#endif

typedef uint64_t lfx2Timestamp;
typedef uint64_t lfx2Interval;

typedef uint32_t lfx2SectionId;

#if defined(LFX2_VK)
typedef struct lfx2VulkanSubmitAux {
  VkCommandBuffer submit_before;
  VkCommandBuffer submit_after;
  VkSemaphore signal_sem;
  uint64_t signal_sem_value;
} lfx2VulkanSubmitAux;
#endif

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

#if (defined(LFX2_DX12) && defined(_WIN32))
LFX2_API struct lfx2Dx12Context *lfx2Dx12ContextCreate(ID3D12Device* device);

LFX2_API void lfx2Dx12ContextAddRef(struct lfx2Dx12Context *context);

LFX2_API void lfx2Dx12ContextRelease(struct lfx2Dx12Context *context);

LFX2_API
struct lfx2Dx12SubmitAux lfx2Dx12ContextBeforeSubmit(struct lfx2Dx12Context *context,
                                                     ID3D12CommandQueue* queue);

LFX2_API void lfx2Dx12ContextBeginFrame(struct lfx2Dx12Context *context, struct lfx2Frame *frame);

LFX2_API void lfx2Dx12ContextEndFrame(struct lfx2Dx12Context *context, struct lfx2Frame *frame);
#endif

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

LFX2_API
void lfx2FrameOverrideQueuingDelay(struct lfx2Frame *frame,
                                   lfx2SectionId section_id,
                                   lfx2Interval queueing_delay);

LFX2_API
void lfx2FrameOverrideInverseThroughput(struct lfx2Frame *frame,
                                        lfx2SectionId section_id,
                                        lfx2Interval inverse_throughput);

LFX2_API struct lfx2ImplicitContext *lfx2ImplicitContextCreate(void);

LFX2_API void lfx2ImplicitContextRelease(struct lfx2ImplicitContext *context);

LFX2_API void lfx2ImplicitContextReset(struct lfx2ImplicitContext *context);

LFX2_API
struct lfx2Frame *lfx2FrameCreateImplicit(struct lfx2ImplicitContext *context,
                                          lfx2Timestamp *out_timestamp);

LFX2_API
struct lfx2Frame *lfx2FrameDequeueImplicit(struct lfx2ImplicitContext *context,
                                           bool critical);

#if defined(LFX2_VK)
LFX2_API
struct lfx2VulkanContext *lfx2VulkanContextCreate(PFN_vkGetInstanceProcAddr gipa,
                                                  VkInstance instance,
                                                  VkPhysicalDevice physical_device,
                                                  VkDevice device,
                                                  uint32_t queue_family_index);

LFX2_API void lfx2VulkanContextAddRef(struct lfx2VulkanContext *context);

LFX2_API void lfx2VulkanContextRelease(struct lfx2VulkanContext *context);

LFX2_API
struct lfx2VulkanSubmitAux lfx2VulkanContextBeforeSubmit(struct lfx2VulkanContext *context);

LFX2_API
void lfx2VulkanContextBeginFrame(struct lfx2VulkanContext *context,
                                 struct lfx2Frame *frame);

LFX2_API void lfx2VulkanContextEndFrame(struct lfx2VulkanContext *context, struct lfx2Frame *frame);
#endif

#ifdef __cplusplus
} // extern "C"
#endif // __cplusplus

#endif /* LATENCYFLEX2_H */
