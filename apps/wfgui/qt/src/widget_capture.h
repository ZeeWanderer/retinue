#pragma once

#include <QPixmap>
#include <QString>
#include <QStringList>

class QWidget;

namespace wfgui {

void setCaptureTarget(QWidget *widget, const QString &name,
                      bool containsItems = false);
void setCaptureItem(QWidget *widget);

QStringList captureTargetNames();
QPixmap grabCaptureTarget(const QString &name, int padding, QString *error);

} // namespace wfgui
