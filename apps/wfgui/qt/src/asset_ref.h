#pragma once

#include <QHash>
#include <QMetaType>
#include <QString>

class QJsonObject;

namespace wfgui {

struct AssetRef {
  QString id;
  QString source;
  QString imageName;
  QString path;
  QString digest;
  QString mediaType;
  qint64 size = 0;
  bool stale = false;

  [[nodiscard]] bool isValid() const;
  [[nodiscard]] bool isPersistent() const;

  static AssetRef fromJson(const QJsonObject &value);
  static AssetRef embedded(QString id, QString path);

  bool operator==(const AssetRef &) const = default;
};

using AssetMap = QHash<QString, AssetRef>;

} // namespace wfgui

Q_DECLARE_METATYPE(wfgui::AssetRef)
