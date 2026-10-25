#include "activity_rail_widget.h"
#include "thumbnail_widget.h"

#include <QAbstractButton>
#include <QButtonGroup>
#include <QDateTime>
#include <QFrame>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QJsonArray>
#include <QLabel>
#include <QMap>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QPixmap>
#include <QPropertyAnimation>
#include <QPushButton>
#include <QScrollArea>
#include <QSizePolicy>
#include <QStackedWidget>
#include <QTimer>
#include <QVBoxLayout>

#include <algorithm>

#include "activity_data.h"
#include "app_controller.h"
#include "market_rail_widget.h"
#include "widget_capture.h"

namespace {
class CapsuleWidget final : public QWidget {
public:
  explicit CapsuleWidget(const QColor &color) : color_(color) {}

protected:
  void paintEvent(QPaintEvent *) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(Qt::NoPen);
    painter.setBrush(color_);
    const QRectF bounds = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
    painter.drawRoundedRect(bounds, bounds.height() / 2.0,
                            bounds.height() / 2.0);
  }

private:
  QColor color_;
};

class NotificationModeSwitch final : public QAbstractButton {
public:
  explicit NotificationModeSwitch(QWidget *parent = nullptr)
      : QAbstractButton(parent) {
    setObjectName("notificationMode");
    setFixedSize(48, 17);
    setCursor(Qt::PointingHandCursor);
    setAccessibleName("Fissure notification mode");
    updateDescription();
  }

  QString mode() const { return mode_; }

  void setMode(const QString &mode) {
    if (mode_ == mode) {
      return;
    }
    mode_ = mode;
    updateDescription();
    update();
  }

protected:
  void paintEvent(QPaintEvent *) override {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setOpacity(isEnabled() ? 1.0 : 0.45);
    const QRectF track(0.5, 0.5, width() - 1.0, height() - 1.0);
    QColor trackColor("#4b5265");
    if (mode_ == "session") {
      trackColor = QColor("#7653df");
    } else if (mode_ == "persistent") {
      trackColor = QColor("#4aa57b");
    }
    painter.setPen(Qt::NoPen);
    painter.setBrush(trackColor);
    painter.drawRoundedRect(track, 8, 8);

    constexpr qreal stops[] = {8.5, 24.0, 39.5};
    painter.setBrush(QColor(255, 255, 255, 95));
    for (qreal stop : stops) {
      painter.drawEllipse(QPointF(stop, 8.5), 1.5, 1.5);
    }
    const int index = mode_ == "persistent" ? 2 : mode_ == "session" ? 1 : 0;
    painter.setBrush(QColor("#f6f7fb"));
    painter.drawEllipse(QPointF(stops[index], 8.5), 6.5, 6.5);
  }

private:
  void updateDescription() {
    QString description = "Off";
    if (mode_ == "session") {
      description = "On while the GUI is open";
    } else if (mode_ == "persistent") {
      description = "Persistent daemon notifications";
    }
    setToolTip("Fissure notifications: " + description);
    setAccessibleDescription(description);
  }

  QString mode_ = "off";
};

void clearLayout(QLayout *layout) {
  while (QLayoutItem *item = layout->takeAt(0)) {
    delete item->widget();
    delete item;
  }
}

qint64 isoTime(const QString &value) {
  const QDateTime time = QDateTime::fromString(value, Qt::ISODate);
  return time.isValid() ? time.toMSecsSinceEpoch() : 0;
}

QString remaining(qint64 expiresAt) {
  const qint64 remainingMs =
      std::max<qint64>(0, expiresAt - QDateTime::currentMSecsSinceEpoch());
  qint64 seconds = (remainingMs + 999) / 1000;
  const qint64 days = seconds / 86400;
  seconds %= 86400;
  const qint64 hours = seconds / 3600;
  seconds %= 3600;
  const qint64 minutes = seconds / 60;
  seconds %= 60;
  if (days > 0) {
    return QString("%1d %2h").arg(days).arg(hours);
  }
  if (hours > 0) {
    return QString("%1h %2m").arg(hours).arg(minutes);
  }
  return QString("%1m %2s").arg(minutes).arg(seconds);
}

QString cycleIcon(const QString &id) {
  if (id == "earth" || id == "cetus") {
    return ":/resources/activity/earth.png";
  }
  if (id == "vallis") {
    return ":/resources/activity/vallis.png";
  }
  return ":/resources/activity/cambion.png";
}

QString tierIcon(const QString &tier) {
  const QString name = tier.toLower();
  static const QMap<QString, QString> icons = {
      {"lith", ":/resources/activity/lith.png"},
      {"meso", ":/resources/activity/meso.png"},
      {"neo", ":/resources/activity/neo.png"},
      {"axi", ":/resources/activity/axi.png"},
      {"requiem", ":/resources/activity/requiem.png"},
      {"omnia", ":/resources/activity/omnia.png"},
  };
  return icons.value(name, ":/resources/ui/nav_relic.png");
}

QPixmap tintedPixmap(const QString &path, const QColor &color) {
  const QPixmap source(path);
  QPixmap result(source.size());
  result.fill(Qt::transparent);
  QPainter painter(&result);
  painter.drawPixmap(0, 0, source);
  painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
  painter.fillRect(result.rect(), color);
  return result;
}

QIcon maskedIcon(const QString &path) {
  QIcon icon;
  icon.addPixmap(tintedPixmap(path, QColor("#aeb5c7")), QIcon::Normal,
                 QIcon::Off);
  icon.addPixmap(tintedPixmap(path, QColor("#d8dbea")), QIcon::Active,
                 QIcon::Off);
  icon.addPixmap(tintedPixmap(path, Qt::white), QIcon::Normal, QIcon::On);
  return icon;
}

ThumbnailWidget *imageLabel(const QString &path, int frameSize,
                            const char *objectName, int imageSize = 0,
                            const QColor &tint = {}) {
  auto *label = new ThumbnailWidget;
  label->setObjectName(objectName);
  label->setFixedSize(frameSize, frameSize);
  const int targetSize = imageSize > 0 ? imageSize : frameSize;
  label->setImageBounds(QSize(targetSize, targetSize));
  label->setAsset(wfgui::AssetRef::embedded(path, path));
  label->setTint(tint);
  return label;
}

QPushButton *railTab(const QString &iconPath, const QString &text,
                     bool selected) {
  auto *tab = new QPushButton(text);
  tab->setObjectName("railTab");
  tab->setCheckable(true);
  tab->setChecked(selected);
  tab->setIcon(maskedIcon(iconPath));
  tab->setIconSize({18, 18});
  tab->setCursor(Qt::PointingHandCursor);
  tab->setFixedHeight(33);
  return tab;
}

QWidget *cycleCard(const QJsonObject &cycle) {
  auto *card = new QWidget;
  card->setObjectName("cycleCard");
  card->setFixedHeight(50);
  card->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  auto *layout = new QHBoxLayout(card);
  layout->setContentsMargins(1, 1, 12, 1);
  layout->setSpacing(7);
  layout->addWidget(
      imageLabel(cycleIcon(cycle.value("id").toString()), 48, "cycleIcon", 30));

  auto *copy = new QVBoxLayout;
  copy->setContentsMargins(0, 4, 0, 4);
  copy->setSpacing(1);
  auto *top = new QHBoxLayout;
  top->setSpacing(5);
  auto *title = new QLabel(cycle.value("name").toString().toUpper());
  title->setObjectName("cycleName");
  auto *state = new QLabel(cycle.value("state").toString().toUpper());
  state->setObjectName("cycleState");
  top->addStretch();
  top->addWidget(title);
  top->addWidget(state);
  top->addStretch();
  copy->addLayout(top);
  auto *countdown = new QLabel;
  countdown->setObjectName("countdown");
  countdown->setAlignment(Qt::AlignCenter);
  countdown->setProperty("expiresAt", cycle.value("expires_at").toInteger());
  copy->addWidget(countdown);
  layout->addLayout(copy, 1);
  return card;
}

QWidget *eventPill(const QString &iconPath, QLabel *label,
                   const char *objectName, const QColor &color) {
  auto *pill = new CapsuleWidget(color);
  pill->setObjectName(objectName);
  pill->setFixedHeight(25);
  auto *layout = new QHBoxLayout(pill);
  layout->setContentsMargins(8, 3, 8, 3);
  layout->setSpacing(4);
  if (!iconPath.isEmpty()) {
    layout->addWidget(imageLabel(iconPath, 17, "eventIcon"));
  }
  label->setAlignment(Qt::AlignCenter);
  layout->addWidget(label);
  return pill;
}

void setEvent(QLabel *label, const QString &kind, const QString &title,
              const QString &detail, const QJsonObject &event) {
  label->setProperty("eventKind", kind);
  label->setProperty("eventTitle", title);
  label->setProperty("eventDetail", detail);
  label->setProperty("startsAt", isoTime(event.value("activation").toString()));
  label->setProperty("expiresAt", isoTime(event.value("expiry").toString()));
  label->setProperty("available", !event.isEmpty());
}

void updateEvent(QLabel *label) {
  const QString kind = label->property("eventKind").toString();
  const QString title = label->property("eventTitle").toString();
  if (!label->property("available").toBool()) {
    label->setText(kind == "baro" ? "Baro --" : title);
    label->setToolTip("Schedule unavailable");
    return;
  }
  const QString detail = label->property("eventDetail").toString();
  const qint64 startsAt = label->property("startsAt").toLongLong();
  const qint64 expiresAt = label->property("expiresAt").toLongLong();
  const qint64 now = QDateTime::currentMSecsSinceEpoch();
  QString timing;
  if (startsAt > now) {
    timing = "in " + remaining(startsAt);
  } else if (expiresAt > now) {
    timing = remaining(expiresAt) + " left";
  }
  label->setText(
      kind == "baro"
          ? QString("Baro%1").arg(timing.isEmpty() ? QString() : " " + timing)
          : title);
  label->setToolTip(title + (detail.isEmpty() ? QString() : "\n" + detail) +
                    (timing.isEmpty() ? QString() : "\n" + timing));
}

QString fissureLocation(const QString &node) {
  const int planet = node.lastIndexOf(" (");
  if (planet < 0 || !node.endsWith(')')) {
    return node;
  }
  return node.left(planet) + ", " +
         node.mid(planet + 2, node.size() - planet - 3);
}

} // namespace

ActivityRailWidget::ActivityRailWidget(AppController *controller,
                                       QWidget *parent)
    : QWidget(parent), controller_(controller), cycles_(new QGridLayout),
      fissures_(new QVBoxLayout), resurgence_(new QLabel), baro_(new QLabel),
      status_(new QLabel),
      timersTab_(railTab(":/resources/ui/world.png", "Timers && Events", true)),
      marketTab_(railTab(":/resources/ui/market.png", "WFMarket", false)),
      pages_(new QStackedWidget), market_(new MarketRailWidget(controller)) {
  setObjectName("activityRail");
  wfgui::setCaptureTarget(this, "right-rail");
  setAttribute(Qt::WA_StyledBackground, true);
  setFixedWidth(400);

  auto *layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(0);

  auto *header = new QWidget;
  header->setObjectName("railTabHeader");
  wfgui::setCaptureTarget(header, "right-rail.tabs");
  auto *headerLayout = new QHBoxLayout(header);
  headerLayout->setContentsMargins(7, 0, 7, 0);
  headerLayout->setSpacing(1);
  auto *tabs = new QButtonGroup(this);
  tabs->setExclusive(true);
  tabs->addButton(timersTab_, 0);
  tabs->addButton(marketTab_, 1);
  headerLayout->addWidget(timersTab_, 1);
  headerLayout->addWidget(marketTab_, 1);
  layout->addWidget(header);

  auto *body = new QWidget;
  body->setObjectName("activityBody");
  wfgui::setCaptureTarget(body, "right-rail.timers");
  auto *bodyLayout = new QVBoxLayout(body);
  bodyLayout->setContentsMargins(4, 8, 4, 0);
  bodyLayout->setSpacing(0);

  auto *events = new QHBoxLayout;
  events->setContentsMargins(3, 0, 3, 0);
  events->setSpacing(0);
  events->addStretch();
  resurgence_->setObjectName("resurgenceStatus");
  events->addWidget(eventPill(":/resources/activity/aya.png", resurgence_,
                              "resurgencePill", QColor("#432289")));
  events->addSpacing(20);
  baro_->setObjectName("baroStatus");
  events->addWidget(eventPill(QString(), baro_, "baroPill", QColor("#20283e")));
  events->addStretch();
  bodyLayout->addLayout(events);
  bodyLayout->addSpacing(7);

  cycles_->setContentsMargins(0, 0, 0, 0);
  cycles_->setHorizontalSpacing(6);
  cycles_->setVerticalSpacing(6);
  cycles_->setColumnStretch(0, 1);
  cycles_->setColumnStretch(1, 1);
  bodyLayout->addLayout(cycles_);
  bodyLayout->addSpacing(7);

  auto *fissureSection = new QWidget;
  fissureSection->setObjectName("fissureSection");
  wfgui::setCaptureTarget(fissureSection, "right-rail.timers.fissures");
  auto *fissureSectionLayout = new QVBoxLayout(fissureSection);
  fissureSectionLayout->setContentsMargins(0, 0, 0, 0);
  fissureSectionLayout->setSpacing(0);

  auto *fissureHeader = new QWidget;
  fissureHeader->setObjectName("fissureHeader");
  auto *fissureHeaderLayout = new QHBoxLayout(fissureHeader);
  fissureHeaderLayout->setContentsMargins(8, 2, 4, 2);
  fissureHeaderLayout->setSpacing(2);
  auto *fissureTitle = new QLabel("Void fissures");
  fissureTitle->setObjectName("sectionTitle");
  fissureHeaderLayout->addWidget(fissureTitle);
  fissureHeaderLayout->addStretch();
  auto *notificationControl = new QWidget;
  notificationControl->setObjectName("notificationControl");
  wfgui::setCaptureTarget(notificationControl,
                          "right-rail.timers.notifications");
  auto *notificationLayout = new QHBoxLayout(notificationControl);
  notificationLayout->setContentsMargins(0, 0, 5, 0);
  notificationLayout->setSpacing(4);
  notificationLayout->addWidget(imageLabel(":/resources/ui/notification.png",
                                           17, "fissureNotificationIcon", 16,
                                           QColor("#aeb5c7")));
  auto *notificationMode = new NotificationModeSwitch;
  notificationMode->setMode(controller_->fissureNotificationMode());
  notificationMode->setEnabled(controller_->notificationSettingsLoaded());
  notificationLayout->addWidget(notificationMode);
  fissureHeaderLayout->addWidget(notificationControl);
  fissureHeaderLayout->addSpacing(8);
  auto *modes = new QButtonGroup(this);
  modes->setExclusive(true);
  struct Mode {
    const char *label;
    const char *value;
    const char *icon;
  };
  constexpr Mode filters[] = {
      {"All fissures", "all", ":/resources/ui/all.png"},
      {"Normal fissures", "normal", ":/resources/ui/normal.png"},
      {"Steel Path fissures", "steel", ":/resources/ui/steel.png"},
  };
  int id = 0;
  for (const auto &[label, value, icon] : filters) {
    auto *button = new QPushButton;
    button->setObjectName("railFilter");
    button->setCheckable(true);
    button->setChecked(id == 0);
    button->setProperty("mode", value);
    button->setIcon(maskedIcon(icon));
    button->setIconSize({18, 18});
    button->setToolTip(label);
    modes->addButton(button, id++);
    fissureHeaderLayout->addWidget(button);
  }
  fissureSectionLayout->addWidget(fissureHeader);

  auto *listHost = new QWidget;
  listHost->setObjectName("fissureList");
  fissures_->setContentsMargins(4, 4, 4, 4);
  fissures_->setSpacing(4);
  listHost->setLayout(fissures_);
  auto *scroll = new QScrollArea;
  scroll->setObjectName("activityScroll");
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  scroll->setWidget(listHost);
  fissureSectionLayout->addWidget(scroll, 1);
  bodyLayout->addWidget(fissureSection, 1);

  status_->setObjectName("secondaryText");
  status_->setContentsMargins(0, 7, 0, 0);
  status_->setMaximumHeight(0);
  status_->setWordWrap(true);
  bodyLayout->addWidget(status_);
  pages_->setObjectName("activityPages");
  pages_->addWidget(body);
  pages_->addWidget(market_);
  layout->addWidget(pages_, 1);

  statusAnimation_ = new QPropertyAnimation(status_, "maximumHeight", this);
  statusAnimation_->setDuration(160);
  statusAnimation_->setEasingCurve(QEasingCurve::InOutCubic);
  connect(statusAnimation_, &QPropertyAnimation::finished, this, [this] {
    if (status_->maximumHeight() == 0) {
      status_->clear();
    }
  });

  connect(modes, &QButtonGroup::idClicked, this, [this, modes](int buttonId) {
    mode_ = modes->button(buttonId)->property("mode").toString();
    rebuildFissures(controller_->activity());
    updateCountdowns();
  });
  connect(tabs, &QButtonGroup::idClicked, pages_,
          &QStackedWidget::setCurrentIndex);
  connect(market_, &MarketRailWidget::signInRequested, this,
          &ActivityRailWidget::signInRequested);
  connect(notificationMode, &QAbstractButton::clicked, this,
          [this, notificationMode] {
            const QString next = notificationMode->mode() == "off" ? "session"
                                 : notificationMode->mode() == "session"
                                     ? "persistent"
                                     : "off";
            controller_->setFissureNotificationMode(next);
          });
  connect(controller_, &AppController::notificationSettingsChanged, this,
          [this, notificationMode] {
            notificationMode->setMode(controller_->fissureNotificationMode());
            notificationMode->setEnabled(
                controller_->notificationSettingsLoaded());
          });
  connect(controller_, &AppController::activityStateChanged, this,
          &ActivityRailWidget::rebuild);
  auto *timer = new QTimer(this);
  timer->setInterval(1000);
  connect(timer, &QTimer::timeout, this, &ActivityRailWidget::updateCountdowns);
  timer->start();
  rebuild();
}

bool ActivityRailWidget::setTab(const QString &tab) {
  const QString normalized = tab.trimmed().toLower();
  if (normalized == "timers" || normalized == "events") {
    timersTab_->setChecked(true);
    pages_->setCurrentIndex(0);
    return true;
  }
  if (normalized == "market" || normalized == "wfmarket") {
    marketTab_->setChecked(true);
    pages_->setCurrentIndex(1);
    return true;
  }
  return false;
}

void ActivityRailWidget::showMarketItem(const QString &item,
                                        const QString &side) {
  setTab("market");
  market_->showItem(item, side);
}

bool ActivityRailWidget::eventFilter(QObject *watched, QEvent *event) {
  if (event->type() == QEvent::MouseButtonRelease) {
    const auto *mouse = static_cast<QMouseEvent *>(event);
    QWidget *widget = qobject_cast<QWidget *>(watched);
    while (widget && widget->objectName() != "fissureGroup") {
      widget = widget->parentWidget();
    }
    if (mouse->button() == Qt::LeftButton && widget) {
      emit relicEraRequested(widget->property("relicEra").toString());
      return true;
    }
  }
  return QWidget::eventFilter(watched, event);
}

void ActivityRailWidget::rebuild() {
  const QJsonObject data = controller_->activity();
  rebuildCycles(data);
  rebuildFissures(data);
  const QJsonObject baro = data.value("baro").toObject();
  setEvent(baro_, "baro", "Baro Ki'Teer", baro.value("node").toString(), baro);
  const QJsonObject resurgence = data.value("resurgence").toObject();
  setEvent(resurgence_, "resurgence", "Prime Resurgence",
           resurgence.value("featured").toString(), resurgence);
  setStatus(data.value("error").toString());
  updateCountdowns();
}

void ActivityRailWidget::setStatus(const QString &error) {
  statusAnimation_->stop();
  const int currentHeight = status_->maximumHeight();
  int targetHeight = 0;
  if (!error.isEmpty()) {
    status_->setText(error);
    targetHeight = status_->sizeHint().height();
  }
  if (currentHeight == targetHeight) {
    if (targetHeight == 0) {
      status_->clear();
    }
    return;
  }
  statusAnimation_->setStartValue(currentHeight);
  statusAnimation_->setEndValue(targetHeight);
  statusAnimation_->start();
}

void ActivityRailWidget::rebuildCycles(const QJsonObject &data) {
  clearLayout(cycles_);
  const QJsonArray values = data.value("cycles").toArray();
  for (int index = 0; index < values.size(); ++index) {
    cycles_->addWidget(cycleCard(values.at(index).toObject()), index / 2,
                       index % 2);
  }
}

void ActivityRailWidget::rebuildFissures(const QJsonObject &data) {
  clearLayout(fissures_);
  QMap<QString, QJsonArray> groups;
  const QJsonArray active = wfgui::activeFissures(
      data.value("fissures").toArray(), QDateTime::currentMSecsSinceEpoch());
  for (const QJsonValue &value : active) {
    const QJsonObject fissure = value.toObject();
    const bool hard = fissure.value("hard").toBool();
    if ((mode_ == "normal" && hard) || (mode_ == "steel" && !hard)) {
      continue;
    }
    groups[fissure.value("tier").toString()].append(fissure);
  }
  const QStringList order = {"Lith", "Meso", "Neo", "Axi", "Requiem", "Omnia"};
  QStringList tiers = order;
  for (auto group = groups.cbegin(); group != groups.cend(); ++group) {
    if (!tiers.contains(group.key())) {
      tiers.append(group.key());
    }
  }
  for (const QString &tier : tiers) {
    const QJsonArray rows = groups.value(tier);
    if (rows.isEmpty()) {
      continue;
    }
    auto *group = new QWidget;
    group->setObjectName("fissureGroup");
    group->setProperty("relicEra", wfgui::relicEraForFissureTier(tier));
    group->setCursor(Qt::PointingHandCursor);
    auto *groupLayout = new QHBoxLayout(group);
    groupLayout->setContentsMargins(6, 5, 6, 5);
    groupLayout->setSpacing(8);
    auto *era = imageLabel(tierIcon(tier), 42, "fissureEra");
    era->setToolTip(tier);
    groupLayout->addWidget(era, 0, Qt::AlignVCenter);
    auto *rowsLayout = new QVBoxLayout;
    rowsLayout->setContentsMargins(0, 0, 0, 0);
    rowsLayout->setSpacing(4);
    for (const QJsonValue &value : rows) {
      const QJsonObject fissure = value.toObject();
      auto *row = new QWidget;
      row->setObjectName("fissureRow");
      auto *rowLayout = new QGridLayout(row);
      rowLayout->setContentsMargins(0, 0, 0, 0);
      rowLayout->setHorizontalSpacing(4);
      rowLayout->setColumnStretch(0, 3);
      rowLayout->setColumnStretch(1, 1);

      auto *nameGroup = new QWidget;
      nameGroup->setObjectName("fissureName");
      auto *nameLayout = new QHBoxLayout(nameGroup);
      nameLayout->setContentsMargins(0, 0, 0, 0);
      nameLayout->setSpacing(5);
      nameLayout->addStretch();
      const bool hard = fissure.value("hard").toBool();
      auto *mode = imageLabel(hard ? ":/resources/ui/steel.png"
                                   : ":/resources/ui/normal.png",
                              14, "fissureMode", 14, QColor("#d8dbea"));
      mode->setToolTip(hard ? "Steel Path" : "Normal");
      nameLayout->addWidget(mode);
      const QString missionName = fissure.value("mission").toString();
      auto *mission = new QLabel(missionName);
      mission->setObjectName("fissureMissionType");
      nameLayout->addWidget(mission);
      const QString location =
          fissureLocation(fissure.value("node").toString());
      auto *place = new QLabel(QString("(%1)").arg(location));
      place->setObjectName("fissureLocation");
      nameLayout->addWidget(place);
      nameLayout->addStretch();
      nameGroup->setToolTip(QString("%1 (%2)").arg(missionName, location));
      rowLayout->addWidget(nameGroup, 0, 0);
      auto *countdown = new QLabel;
      countdown->setObjectName("countdown");
      countdown->setProperty("expiresAt",
                             isoTime(fissure.value("expiry").toString()));
      countdown->setProperty("fissureExpiry", true);
      rowLayout->addWidget(countdown, 0, 1, Qt::AlignCenter);
      rowsLayout->addWidget(row);
    }
    groupLayout->addLayout(rowsLayout, 1);
    group->installEventFilter(this);
    for (QWidget *child : group->findChildren<QWidget *>()) {
      child->installEventFilter(this);
    }
    fissures_->addWidget(group);
  }
  fissures_->addStretch();
}

void ActivityRailWidget::updateCountdowns() {
  const qint64 now = QDateTime::currentMSecsSinceEpoch();
  bool rebuildExpiredFissures = false;
  for (QLabel *label : findChildren<QLabel *>("countdown")) {
    const qint64 expiresAt = label->property("expiresAt").toLongLong();
    rebuildExpiredFissures |= label->property("fissureExpiry").toBool() &&
                              expiresAt > 0 && expiresAt <= now;
    label->setText(expiresAt > 0 ? remaining(expiresAt) : "--");
  }
  updateEvent(resurgence_);
  updateEvent(baro_);
  if (rebuildExpiredFissures) {
    rebuildFissures(controller_->activity());
  }
}
