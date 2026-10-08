// Browser-owned adapter to the exactly pinned, existing BSD-3-Clause ANGLE header.
// ANGLE checks all public method names before returning its provider-owned struct.
// No copied third-party implementation, native context, or author pointer crosses here.
#include <platform/PlatformMethods.h>

extern "C" bool breeze_angle_set_worker_delegate(
    void *display, const void *entry, angle::PostWorkerTaskFunc post) noexcept {
    if (!display || !entry || !post) return false;
    // A fixed eglGetProcAddress result; the header supplies its actual calling convention.
    auto get = reinterpret_cast<angle::GetDisplayPlatformFunc>(const_cast<void *>(entry));
    angle::PlatformMethods *methods = nullptr;
    if (!get(display, angle::g_PlatformMethodNames, angle::g_NumPlatformMethods,
             nullptr, &methods) || !methods) return false;
    methods->postWorkerTask = post;
    return true;
}
