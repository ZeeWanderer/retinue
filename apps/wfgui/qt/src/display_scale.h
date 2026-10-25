#pragma once

namespace wfgui {

inline constexpr int MinimumUiScalePercent = 25;
inline constexpr int MaximumUiScalePercent = 175;
inline constexpr int UiScaleStepPercent = 5;

int normalizedUiScalePercent(int percent);
int configuredUiScalePercent();
void setConfiguredUiScalePercent(int percent);
void applyConfiguredUiScale();

} // namespace wfgui
