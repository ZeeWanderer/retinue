#include "tooltip.h"

#include <QApplication>
#include <QEvent>
#include <QHelpEvent>
#include <QLabel>
#include <QMouseEvent>
#include <QPointer>
#include <QTimer>
#include <QWidget>

#include <algorithm>

namespace {
class TooltipFilter final : public QObject {
public:
  explicit TooltipFilter(QObject *parent) : QObject(parent) {
    timer_.setSingleShot(true);
    connect(&timer_, &QTimer::timeout, this, &TooltipFilter::hide);
  }

  void show(QWidget *source, const QPoint &localPosition, const QString &text,
            const QRect &rect, int duration) {
    if (source == nullptr || text.isEmpty()) {
      hide();
      return;
    }
    QWidget *window = source->window();
    if (window == nullptr) {
      hide();
      return;
    }
    if (label_ == nullptr || label_->parentWidget() != window) {
      if (label_ != nullptr) {
        delete label_;
      }
      label_ = new QLabel(window);
      label_->setObjectName("wfguiTooltip");
      label_->setAttribute(Qt::WA_TransparentForMouseEvents);
      label_->setTextFormat(Qt::PlainText);
      label_->setWordWrap(true);
      label_->setMaximumWidth(360);
    }

    source_ = source;
    rect_ = rect.isValid() ? rect : source->rect();
    label_->setText(text);
    label_->adjustSize();

    const QPoint anchor = source->mapTo(window, localPosition);
    constexpr int OffsetX = 12;
    constexpr int OffsetY = 18;
    constexpr int Margin = 6;
    QPoint position = anchor + QPoint(OffsetX, OffsetY);
    if (position.x() + label_->width() > window->width() - Margin) {
      position.setX(anchor.x() - label_->width() - OffsetX);
    }
    if (position.y() + label_->height() > window->height() - Margin) {
      position.setY(anchor.y() - label_->height() - OffsetY);
    }
    position.setX(
        qBound(Margin, position.x(),
               std::max(Margin, window->width() - label_->width() - Margin)));
    position.setY(
        qBound(Margin, position.y(),
               std::max(Margin, window->height() - label_->height() - Margin)));
    label_->move(position);
    label_->show();
    label_->raise();
    timer_.start(duration > 0 ? duration : 10000);
  }

  void hide() {
    timer_.stop();
    source_.clear();
    rect_ = {};
    if (label_ != nullptr) {
      label_->hide();
    }
  }

protected:
  bool eventFilter(QObject *watched, QEvent *event) override {
    if (event->type() == QEvent::ToolTip) {
      auto *widget = qobject_cast<QWidget *>(watched);
      if (widget == nullptr || widget->toolTip().isEmpty()) {
        return false;
      }
      const auto *help = static_cast<QHelpEvent *>(event);
      show(widget, help->pos(), widget->toolTip(), widget->rect(),
           widget->toolTipDuration());
      return true;
    }

    if (watched == source_) {
      if (event->type() == QEvent::Leave || event->type() == QEvent::Hide ||
          event->type() == QEvent::Close ||
          event->type() == QEvent::MouseButtonPress ||
          event->type() == QEvent::Wheel) {
        hide();
      } else if (event->type() == QEvent::MouseMove && !rect_.isNull() &&
                 !rect_.contains(
                     static_cast<QMouseEvent *>(event)->position().toPoint())) {
        hide();
      }
    } else if (event->type() == QEvent::ApplicationDeactivate) {
      hide();
    }
    return false;
  }

private:
  QPointer<QLabel> label_;
  QPointer<QWidget> source_;
  QRect rect_;
  QTimer timer_;
};

QPointer<TooltipFilter> tooltipFilter;
} // namespace

namespace wfgui {

void installTooltipHandling(QApplication &application) {
  static constexpr const char *InstalledProperty = "wfguiTooltipHandling";
  if (application.property(InstalledProperty).toBool()) {
    return;
  }
  application.setProperty(InstalledProperty, true);
  tooltipFilter = new TooltipFilter(&application);
  application.installEventFilter(tooltipFilter);
}

void showTooltip(QWidget *widget, const QPoint &localPosition,
                 const QString &text, const QRect &rect, int duration) {
  if (tooltipFilter == nullptr) {
    auto *application = qobject_cast<QApplication *>(QApplication::instance());
    if (application != nullptr) {
      installTooltipHandling(*application);
    }
  }
  if (tooltipFilter != nullptr) {
    tooltipFilter->show(widget, localPosition, text, rect, duration);
  }
}

void hideTooltip() {
  if (tooltipFilter != nullptr) {
    tooltipFilter->hide();
  }
}

} // namespace wfgui
