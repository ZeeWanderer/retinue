#include "title_bar_widget.h"

#include <QHBoxLayout>
#include <QIcon>
#include <QLabel>
#include <QMouseEvent>
#include <QPaintEvent>
#include <QPainter>
#include <QPixmap>
#include <QRadialGradient>
#include <QToolButton>
#include <QWindow>

#include "display_scale.h"

namespace {
QIcon tintedIcon(const QString &path) {
  const QPixmap source(path);
  QPixmap result(source.size());
  result.fill(Qt::transparent);
  QPainter painter(&result);
  painter.drawPixmap(0, 0, source);
  painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
  painter.fillRect(result.rect(), QColor("#d8dbea"));
  return QIcon(result);
}
} // namespace

TitleBarWidget::TitleBarWidget(QWidget *parent)
    : QWidget(parent), leftRail_(new QToolButton), rightRail_(new QToolButton),
      maximize_(new QToolButton) {
  setObjectName("titleBar");
  setFixedHeight(40);

  auto *layout = new QHBoxLayout(this);
  layout->setContentsMargins(7, 5, 10, 5);
  layout->setSpacing(0);
  auto *icon = new QLabel;
  icon->setFixedSize(26, 26);
  icon->setPixmap(QIcon(":/resources/ui/nav_mastery.png").pixmap(24, 24));
  icon->setAlignment(Qt::AlignCenter);
  layout->addWidget(icon);
  layout->addSpacing(3);
  leftRail_->setObjectName("titleRailToggle");
  leftRail_->setIcon(tintedIcon(":/resources/ui/panel-left.png"));
  leftRail_->setIconSize({16, 16});
  leftRail_->setToolTip("Hide navigation rail");
  layout->addWidget(leftRail_);
  layout->addSpacing(5);
  auto *title = new QLabel("wfcli");
  title->setObjectName("windowTitle");
  layout->addWidget(title);
  layout->addStretch();

  auto *scaleDown = new QToolButton;
  auto *scaleValue =
      new QLabel(QString("%1%").arg(wfgui::configuredUiScalePercent()));
  auto *scaleUp = new QToolButton;
  rightRail_->setObjectName("titleRailToggle");
  rightRail_->setIcon(tintedIcon(":/resources/ui/panel-right.png"));
  rightRail_->setIconSize({16, 16});
  rightRail_->setToolTip("Hide activity rail");
  scaleDown->setIcon(tintedIcon(":/resources/ui/zoomout.png"));
  scaleUp->setIcon(tintedIcon(":/resources/ui/zoomin.png"));
  scaleDown->setIconSize({14, 14});
  scaleUp->setIconSize({14, 14});
  scaleDown->setObjectName("titleScaleStep");
  scaleUp->setObjectName("titleScaleStep");
  scaleValue->setObjectName("titleScaleValue");
  scaleValue->setAlignment(Qt::AlignCenter);
  scaleDown->setToolTip("Decrease UI scale");
  scaleUp->setToolTip("Increase UI scale");
  scaleDown->setEnabled(wfgui::configuredUiScalePercent() >
                        wfgui::MinimumUiScalePercent);
  scaleUp->setEnabled(wfgui::configuredUiScalePercent() <
                      wfgui::MaximumUiScalePercent);
  layout->addWidget(scaleDown);
  layout->addWidget(scaleValue);
  layout->addWidget(scaleUp);
  layout->addSpacing(14);
  layout->addWidget(rightRail_);
  layout->addSpacing(7);

  auto *minimize = new QToolButton;
  auto *close = new QToolButton;
  minimize->setObjectName("windowControl");
  maximize_->setObjectName("windowControl");
  close->setObjectName("windowClose");
  minimize->setIcon(tintedIcon(":/resources/ui/window-minimize.png"));
  maximize_->setIcon(tintedIcon(":/resources/ui/window-maximize.png"));
  close->setIcon(tintedIcon(":/resources/ui/window-close.png"));
  minimize->setIconSize({30, 30});
  maximize_->setIconSize({30, 30});
  close->setIconSize({30, 30});
  minimize->setToolTip("Minimize");
  maximize_->setToolTip("Maximize");
  close->setToolTip("Close");
  layout->addWidget(minimize);
  layout->addWidget(maximize_);
  layout->addWidget(close);

  connect(scaleDown, &QToolButton::clicked, this,
          [this] { emit uiScaleDeltaRequested(-wfgui::UiScaleStepPercent); });
  connect(scaleUp, &QToolButton::clicked, this,
          [this] { emit uiScaleDeltaRequested(wfgui::UiScaleStepPercent); });
  connect(leftRail_, &QToolButton::clicked, this,
          &TitleBarWidget::leftRailToggleRequested);
  connect(rightRail_, &QToolButton::clicked, this,
          &TitleBarWidget::rightRailToggleRequested);
  connect(minimize, &QToolButton::clicked, this,
          [this] { window()->showMinimized(); });
  connect(maximize_, &QToolButton::clicked, this,
          &TitleBarWidget::toggleMaximized);
  connect(close, &QToolButton::clicked, this, [this] { window()->close(); });
}

void TitleBarWidget::setLeftRailCollapsed(bool collapsed) {
  leftRail_->setToolTip(collapsed ? "Show navigation rail"
                                  : "Hide navigation rail");
}

void TitleBarWidget::setRightRailCollapsed(bool collapsed) {
  rightRail_->setToolTip(collapsed ? "Show activity rail"
                                   : "Hide activity rail");
}

void TitleBarWidget::setRightRailAvailable(bool available) {
  rightRail_->setEnabled(available);
}

void TitleBarWidget::setMaximized(bool maximized) {
  maximize_->setIcon(tintedIcon(maximized
                                    ? ":/resources/ui/window-restore.png"
                                    : ":/resources/ui/window-maximize.png"));
  maximize_->setToolTip(maximized ? "Restore" : "Maximize");
}

void TitleBarWidget::paintEvent(QPaintEvent *) {
  QPainter painter(this);
  painter.fillRect(rect(), QColor("#101623"));
  QRadialGradient glow(QPointF(7.0, 3.0), 428.0);
  glow.setColorAt(0.0, QColor(53, 86, 179, 26));
  glow.setColorAt(1.0, QColor(96, 215, 223, 0));
  painter.fillRect(rect(), glow);
}

void TitleBarWidget::mousePressEvent(QMouseEvent *event) {
  if (event->button() == Qt::LeftButton && window()->windowHandle()) {
    window()->windowHandle()->startSystemMove();
    event->accept();
    return;
  }
  QWidget::mousePressEvent(event);
}

void TitleBarWidget::mouseDoubleClickEvent(QMouseEvent *event) {
  if (event->button() == Qt::LeftButton) {
    toggleMaximized();
    event->accept();
    return;
  }
  QWidget::mouseDoubleClickEvent(event);
}

void TitleBarWidget::toggleMaximized() {
  window()->isMaximized() ? window()->showNormal() : window()->showMaximized();
}
