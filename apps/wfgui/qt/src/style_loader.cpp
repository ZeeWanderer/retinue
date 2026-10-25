#include "style_loader.h"

#include <QFile>

namespace wfgui {

QString applicationStyleSheet() {
  static constexpr const char *Files[] = {
      ":/resources/styles/foundation.qss", ":/resources/styles/controls.qss",
      ":/resources/styles/content.qss",    ":/resources/styles/activity.qss",
      ":/resources/styles/market.qss",
  };
  QString result;
  for (const char *path : Files) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
      qWarning("could not load GUI stylesheet: %s", path);
      continue;
    }
    result += QString::fromUtf8(file.readAll());
    result += '\n';
  }
  return result;
}

} // namespace wfgui
