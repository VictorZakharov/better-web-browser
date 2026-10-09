// Browser embedding operations absent from rusty_v8 152.2's Rust surface.
// Compile against that exact dependency's public headers;
// do not duplicate V8 layouts or declare private/mangled C++ symbols in Rust.
#include "v8.h"

extern "C" bool breeze_v8_capture_incumbent(v8::Object* result) {
  auto* isolate = v8::Isolate::GetCurrent();
  if (!isolate) return false;
  auto context = isolate->GetIncumbentContext();
  if (context.IsEmpty()) return false;
  auto key = v8::Private::ForApi(isolate, v8::String::NewFromUtf8Literal(isolate, "Breeze.Incumbent"));
  return result->SetPrivate(isolate->GetCurrentContext(), key, context->Global()).FromMaybe(false);
}

extern "C" void breeze_v8_detach_global(v8::Context* context) {
  context->DetachGlobal();
}

extern "C" bool breeze_v8_set_stack_limit(uintptr_t limit) {
  auto* isolate = v8::Isolate::GetCurrent();
  if (!isolate || !limit) return false;
  // Public API updates the active thread's guard, preserving pending interrupts.
  isolate->SetStackLimit(limit);
  return true;
}
