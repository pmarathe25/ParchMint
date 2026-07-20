#include "pdf_renderer.h"

#include <QAbstractTextDocumentLayout>
#include <QPageLayout>
#include <QPageSize>
#include <QPainter>
#include <QPdfWriter>
#include <QTextDocument>

namespace {
constexpr double kMicrometresPerMillimetre = 1000.0;
double millimetres(int micrometres) {
  return static_cast<double>(micrometres) / kMicrometresPerMillimetre;
}
}  // namespace

bool parchmint_render_pdf_qt(const QString& destination,
                             const QString& text,
                             int width_micrometres,
                             int height_micrometres,
                             int margin_left_micrometres,
                             int margin_top_micrometres,
                             int margin_right_micrometres,
                             int margin_bottom_micrometres) {
  QPdfWriter writer(destination);
  writer.setResolution(144);
  writer.setPageSize(QPageSize(
      QSizeF(millimetres(width_micrometres), millimetres(height_micrometres)),
      QPageSize::Millimeter));
  writer.setPageMargins(QMarginsF(millimetres(margin_left_micrometres),
                                  millimetres(margin_top_micrometres),
                                  millimetres(margin_right_micrometres),
                                  millimetres(margin_bottom_micrometres)),
                        QPageLayout::Millimeter);
  QTextDocument document;
  document.setDocumentMargin(0);
  document.setPlainText(text);
  const QRect page = writer.pageLayout().paintRectPixels(writer.resolution());
  document.setPageSize(page.size());

  QPainter painter(&writer);
  if (!painter.isActive()) {
    return false;
  }
  const qreal document_height = document.documentLayout()->documentSize().height();
  const qreal page_height = page.height();
  for (qreal offset = 0; offset < document_height || offset == 0; offset += page_height) {
    if (offset > 0 && !writer.newPage()) {
      painter.end();
      return false;
    }
    painter.save();
    painter.translate(page.left(), page.top() - offset);
    QAbstractTextDocumentLayout::PaintContext context;
    context.clip = QRectF(0, offset, page.width(), page_height);
    document.documentLayout()->draw(&painter, context);
    painter.restore();
  }
  painter.end();
  return true;
}
