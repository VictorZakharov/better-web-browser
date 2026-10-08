// Optional public-V8 CPU sampling. No function names, URLs, source strings,
// author objects or V8 handle layouts cross this C ABI.
#include "v8.h"
#include "v8-profiler.h"
#include <algorithm>
#include <array>
#include <cstdint>
#include <vector>

namespace {
constexpr size_t kRows = 16;
constexpr size_t kCandidates = 128;
constexpr size_t kNodes = 16384;
struct Row { int32_t script, line, column; uint32_t hits; };
struct Summary {
  uint32_t total_hits, non_script_hits, omitted_hits, truncated;
  uint32_t visited, rows, interval_us, reserved;
  uint32_t source_hits[6];
  Row entries[kRows];
};
static_assert(sizeof(Row) == 16 && sizeof(Summary) == 312);
struct Recording {
  v8::CpuProfiler* profiler;
};

void Summarize(const v8::CpuProfileNode* root, Summary* output) {
  std::array<Row, kCandidates> candidates{};
  size_t count = 0;
  std::vector<const v8::CpuProfileNode*> pending;
  pending.reserve(256);
  pending.push_back(root);
  while (!pending.empty() && output->visited < kNodes) {
    auto* node = pending.back(); pending.pop_back();
    ++output->visited;
    const auto hits = node->GetHitCount();
    output->total_hits += hits;
    // Public V8 SourceType: script/builtin/callback/internal/unresolved.
    // No names or strings are read to classify a node. Reserve an explicit
    // other bin instead of assuming future enum values fit this fixed ABI.
    const auto source = static_cast<int>(node->GetSourceType());
    output->source_hits[source >= 0 && source < 5 ? source : 5] += hits;
    const int script = node->GetScriptId();
    if (hits && script > 0 && node->GetLineNumber() > 0) {
      Row row{script, node->GetLineNumber(), node->GetColumnNumber(), hits};
      auto found = std::find_if(candidates.begin(), candidates.begin() + count,
          [&](const Row& old) { return old.script == row.script && old.line == row.line && old.column == row.column; });
      if (found != candidates.begin() + count) found->hits += hits;
      else if (count < kCandidates) candidates[count++] = row;
      else output->omitted_hits += hits;
    } else output->non_script_hits += hits;
    const auto children = node->GetChildrenCount();
    for (int i = 0; i < children; ++i) {
      if (pending.size() >= kNodes) { output->truncated = 1; break; }
      pending.push_back(node->GetChild(i));
    }
  }
  if (!pending.empty()) output->truncated = 1;
  std::sort(candidates.begin(), candidates.begin() + count, [](const Row& a, const Row& b) {
    if (a.hits != b.hits) return a.hits > b.hits;
    if (a.script != b.script) return a.script < b.script;
    if (a.line != b.line) return a.line < b.line;
    return a.column < b.column;
  });
  output->rows = static_cast<uint32_t>(std::min(count, kRows));
  for (size_t i = 0; i < count; ++i) {
    if (i < kRows) output->entries[i] = candidates[i];
    else output->omitted_hits += candidates[i].hits;
  }
}
}

extern "C" void* breeze_v8_start_cpu_samples() {
  auto* isolate = v8::Isolate::GetCurrent();
  if (!isolate) return nullptr;
  v8::HandleScope handles(isolate);
  auto* profiler = v8::CpuProfiler::New(isolate);
  profiler->SetSamplingInterval(2000);
  // Avoid Windows' precision busy-wait: these are diagnostic samples, not a
  // measurement clock or proof of time spent in each JavaScript function.
  profiler->SetUsePreciseSampling(false);
  // The expanded overload avoids passing std::unique_ptr across the public
  // prebuilt-V8/MSVC C++ standard-library ABI boundary.
  const auto result = profiler->StartProfiling(v8::String::Empty(isolate), v8::kLeafNodeLineNumbers, true, 2048);
  if (result != v8::CpuProfilingStatus::kStarted) {
    profiler->Dispose(); return nullptr;
  }
  return new Recording{profiler};
}

extern "C" void breeze_v8_stop_cpu_samples(void* opaque, Summary* output) {
  if (output) { *output = {}; output->interval_us = 2000; }
  auto* recording = static_cast<Recording*>(opaque);
  if (!recording) return;
  v8::HandleScope handles(v8::Isolate::GetCurrent());
  auto* profile = recording->profiler->StopProfiling(v8::String::Empty(v8::Isolate::GetCurrent()));
  if (profile) {
    if (output) Summarize(profile->GetTopDownRoot(), output);
    profile->Delete();
  }
  recording->profiler->Dispose();
  delete recording;
}
