// HTML's cross-origin Window/Location boundary. Failed access checks never fall
// through to author properties. The native hook returns caller-realm wrappers;
// exposing a foreign Function would expose its constructor and bypass isolation.
#include "v8.h"

namespace {
using namespace v8;
Local<String> Text(Isolate* isolate, const char* value) {
  return String::NewFromUtf8(isolate, value).ToLocalChecked();
}

bool SameOrigin(Local<Context> accessing, Local<Object> target, Local<Value>) {
  Local<Context> owner;
  return target->GetCreationContext().ToLocal(&owner) &&
         accessing->GetSecurityToken() == owner->GetSecurityToken();
}

MaybeLocal<Value> Invoke(Local<Object> target, const char* operation,
                         Local<Value> property, Local<Value> value = {}) {
  auto* isolate = Isolate::GetCurrent();
  auto caller = isolate->GetCurrentContext();
  Local<Context> owner;
  Local<Value> hook;
  if (target->GetCreationContext().ToLocal(&owner)) {
    Context::Scope entered(owner);
    auto key = Private::ForApi(isolate, Text(isolate, "Breeze.CrossOriginHook"));
    if (!target->GetPrivate(owner, key).ToLocal(&hook)) return {};
  }
  if (hook.IsEmpty() || !hook->IsFunction()) {
    isolate->ThrowException(Exception::Error(Text(isolate, "Cross-origin access denied")));
    return {};
  }
  Local<Value> args[] = {Text(isolate, operation), property, caller->Global(),
                        value.IsEmpty() ? Local<Value>(Undefined(isolate)) : value};
  return hook.As<Function>()->Call(caller, target, 4, args);
}

Intercepted Get(Local<Name> key, const PropertyCallbackInfo<Value>& info) {
  Local<Value> result;
  if (Invoke(info.HolderV2(), "get", key).ToLocal(&result)) info.GetReturnValue().Set(result);
  return Intercepted::kYes;
}
Intercepted SetProperty(Local<Name> key, Local<Value> value, const PropertyCallbackInfo<Boolean>& info) {
  Local<Value> result;
  if (Invoke(info.HolderV2(), "set", key, value).ToLocal(&result)) info.GetReturnValue().Set(true);
  return Intercepted::kYes;
}
Intercepted Descriptor(Local<Name> key, const PropertyCallbackInfo<Value>& info) {
  Local<Value> result;
  if (Invoke(info.HolderV2(), "descriptor", key).ToLocal(&result)) info.GetReturnValue().Set(result);
  return Intercepted::kYes;
}
Intercepted Delete(Local<Name> key, const PropertyCallbackInfo<Boolean>& info) {
  Invoke(info.HolderV2(), "deny", key).IsEmpty();
  return Intercepted::kYes;
}
Intercepted Define(Local<Name> key, const PropertyDescriptor&, const PropertyCallbackInfo<Boolean>& info) {
  Invoke(info.HolderV2(), "deny", key).IsEmpty();
  return Intercepted::kYes;
}
void Keys(const PropertyCallbackInfo<Array>& info) {
  Local<Value> result;
  if (Invoke(info.HolderV2(), "keys", Undefined(info.GetIsolate())).ToLocal(&result) && result->IsArray())
    info.GetReturnValue().Set(result.As<Array>());
}
Local<Name> Index(Isolate* isolate, uint32_t index) {
  return Integer::NewFromUnsigned(isolate, index)->ToString(isolate->GetCurrentContext()).ToLocalChecked();
}
Intercepted IndexGet(uint32_t key, const PropertyCallbackInfo<Value>& info) { return Get(Index(info.GetIsolate(), key), info); }
Intercepted IndexSet(uint32_t key, Local<Value> value, const PropertyCallbackInfo<Boolean>& info) { return SetProperty(Index(info.GetIsolate(), key), value, info); }
Intercepted IndexDescriptor(uint32_t key, const PropertyCallbackInfo<Value>& info) { return Descriptor(Index(info.GetIsolate(), key), info); }
Intercepted IndexDelete(uint32_t key, const PropertyCallbackInfo<Boolean>& info) { return Delete(Index(info.GetIsolate(), key), info); }
Intercepted IndexDefine(uint32_t key, const PropertyDescriptor& desc, const PropertyCallbackInfo<Boolean>& info) { return Define(Index(info.GetIsolate(), key), desc, info); }
void IndexKeys(const PropertyCallbackInfo<Array>& info) { info.GetReturnValue().Set(Array::New(info.GetIsolate())); }
void FailedAccess(Local<Object> target, AccessType, Local<Value>) {
  Invoke(target, "deny", Undefined(Isolate::GetCurrent())).IsEmpty();
}
}

extern "C" void breeze_v8_window_template(v8::ObjectTemplate* target) {
  v8::Isolate::GetCurrent()->SetFailedAccessCheckCallbackFunction(FailedAccess);
  target->SetAccessCheckCallbackAndHandler(SameOrigin,
      v8::NamedPropertyHandlerConfiguration(Get, SetProperty, Descriptor, Delete, Keys, Define),
      v8::IndexedPropertyHandlerConfiguration(IndexGet, IndexSet, IndexDescriptor, IndexDelete, IndexKeys, IndexDefine));
}
