#pragma once

#include <QByteArray>
#include <QString>

namespace wfgui {

QString settingsFilePath();
QString cacheDirectory();
QString derivativeCacheDirectory();
QString daemonSocketPath();
// Persistent roots may be symlinks; leave existing permissions unchanged.
bool ensurePrivateDirectory(const QString &path);
QByteArray pathReportJson();

} // namespace wfgui
