#pragma once

#include <QJsonArray>
#include <QString>

namespace wfgui {

QJsonArray activeFissures(const QJsonArray &fissures, qint64 now);
QString relicEraForFissureTier(const QString &tier);

} // namespace wfgui
