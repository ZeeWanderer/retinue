#pragma once

#include <QWidget>
#include <QtMath>

namespace wfgui {

inline qreal displayScale(const QWidget *) { return 1.0; }

inline int scaled(int value, qreal scale) { return qRound(value * scale); }

} // namespace wfgui
