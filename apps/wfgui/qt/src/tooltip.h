#pragma once

#include <QPoint>
#include <QRect>
#include <QString>

class QApplication;
class QWidget;

namespace wfgui {

void installTooltipHandling(QApplication &application);

// QWidget::setToolTip is handled globally; use this for painted subregions.
void showTooltip(QWidget *widget, const QPoint &localPosition,
                 const QString &text, const QRect &rect = {},
                 int duration = -1);
void hideTooltip();

} // namespace wfgui
