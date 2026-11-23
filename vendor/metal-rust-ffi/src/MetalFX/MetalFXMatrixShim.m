#import <MetalFX/MetalFX.h>
#import <simd/simd.h>

typedef struct __attribute__((aligned(16))) {
    float columns[4][4];
} MetalRustMatrix4x4;

_Static_assert(sizeof(MetalRustMatrix4x4) == sizeof(simd_float4x4), "matrix size mismatch");
_Static_assert(_Alignof(MetalRustMatrix4x4) == _Alignof(simd_float4x4), "matrix alignment mismatch");

void metal_rust_metalfx_get_world_to_view(id<MTLFXTemporalDenoisedScalerBase> object,
                                           MetalRustMatrix4x4 *output) {
    simd_float4x4 value = object.worldToViewMatrix;
    __builtin_memcpy(output, &value, sizeof(value));
}

void metal_rust_metalfx_set_world_to_view(id<MTLFXTemporalDenoisedScalerBase> object,
                                           MetalRustMatrix4x4 value) {
    simd_float4x4 matrix;
    __builtin_memcpy(&matrix, &value, sizeof(matrix));
    object.worldToViewMatrix = matrix;
}

void metal_rust_metalfx_get_view_to_clip(id<MTLFXTemporalDenoisedScalerBase> object,
                                          MetalRustMatrix4x4 *output) {
    simd_float4x4 value = object.viewToClipMatrix;
    __builtin_memcpy(output, &value, sizeof(value));
}

void metal_rust_metalfx_set_view_to_clip(id<MTLFXTemporalDenoisedScalerBase> object,
                                          MetalRustMatrix4x4 value) {
    simd_float4x4 matrix;
    __builtin_memcpy(&matrix, &value, sizeof(matrix));
    object.viewToClipMatrix = matrix;
}
