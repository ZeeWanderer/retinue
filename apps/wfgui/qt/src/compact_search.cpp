#include "compact_search.h"

#include <QHBoxLayout>
#include <QIcon>
#include <QLineEdit>
#include <QSizePolicy>
#include <QToolButton>
#include <QVariantAnimation>

#include <algorithm>

namespace {
constexpr int CollapsedWidth = 40;
constexpr int MinimumExpandedWidth = 174;
constexpr int MinimumEditorWidth = 125;
constexpr int EditorTextPadding = 36;
} // namespace

CompactSearch::CompactSearch(const QString &placeholder, QWidget *parent)
    : QWidget(parent), editor_(new QLineEdit),
      animation_(new QVariantAnimation(this)) {
  setObjectName("expandingSearch");
  setSizePolicy(QSizePolicy::Maximum, QSizePolicy::Fixed);
  auto *layout = new QHBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->setSpacing(2);
  auto *button = new QToolButton;
  button->setObjectName("compactTool");
  button->setIcon(QIcon(":/resources/ui/search.png"));
  button->setIconSize({17, 17});
  button->setToolTip(placeholder);
  layout->addWidget(button);
  editor_->setObjectName("inlineSearch");
  editor_->setPlaceholderText(placeholder);
  editor_->setClearButtonEnabled(true);
  editor_->setFrame(false);
  editor_->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  editor_->hide();
  layout->addWidget(editor_);
  animation_->setDuration(250);
  animation_->setEasingCurve(QEasingCurve::InOutCubic);
  connect(animation_, &QVariantAnimation::valueChanged, this,
          [this](const QVariant &value) {
            preferredWidth_ = value.toInt();
            updateGeometry();
          });
  connect(animation_, &QVariantAnimation::finished, this, [this] {
    if (!expanded_) {
      editor_->hide();
    }
  });
  connect(button, &QToolButton::clicked, this, [this] {
    if (editor_->isVisible() && editor_->text().isEmpty()) {
      collapse();
    } else {
      expand();
    }
  });
  connect(editor_, &QLineEdit::editingFinished, this, [this] {
    if (editor_->text().isEmpty()) {
      collapse();
    }
  });
  connect(editor_, &QLineEdit::textChanged, this, [this] {
    if (expanded_) {
      animateTo(expandedWidth());
    }
  });
}

QLineEdit *CompactSearch::editor() const { return editor_; }

void CompactSearch::setText(const QString &text) {
  editor_->setText(text);
  if (text.isEmpty()) {
    collapse();
  } else {
    expand();
  }
}

QSize CompactSearch::sizeHint() const {
  QSize result = QWidget::sizeHint();
  result.setWidth(preferredWidth_);
  return result;
}

QSize CompactSearch::minimumSizeHint() const {
  QSize result = QWidget::minimumSizeHint();
  result.setWidth(CollapsedWidth);
  return result;
}

void CompactSearch::expand() {
  expanded_ = true;
  editor_->show();
  animateTo(expandedWidth());
  editor_->setFocus(Qt::MouseFocusReason);
}

void CompactSearch::collapse() {
  expanded_ = false;
  animateTo(CollapsedWidth);
}

void CompactSearch::animateTo(int width) {
  animation_->stop();
  animation_->setStartValue(preferredWidth_);
  animation_->setEndValue(width);
  animation_->start();
}

int CompactSearch::expandedWidth() const {
  const QString content =
      editor_->text().isEmpty() ? editor_->placeholderText() : editor_->text();
  const int editorWidth = std::max(
      MinimumEditorWidth,
      editor_->fontMetrics().horizontalAdvance(content) + EditorTextPadding);
  return std::max(MinimumExpandedWidth,
                  CollapsedWidth + layout()->spacing() + editorWidth);
}
