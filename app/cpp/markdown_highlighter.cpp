#include "markdown_highlighter.h"

#include <QRegularExpression>
#include <QTextCharFormat>
#include <QTextDocument>

MarkdownHighlighter::MarkdownHighlighter(QObject* parent)
  : QSyntaxHighlighter(parent)
{
}

QObject* MarkdownHighlighter::textDocument() const { return m_textDocument; }

void MarkdownHighlighter::setTextDocument(QObject* object)
{
  auto* quickDocument = qobject_cast<QQuickTextDocument*>(object);
  if (object && !quickDocument) {
    emit highlighterError(tr("The source editor supplied an unsupported text document."));
    return;
  }
  if (m_textDocument == quickDocument)
    return;
  m_textDocument = quickDocument;
  setDocument(quickDocument ? quickDocument->textDocument() : nullptr);
  emit textDocumentChanged();
}

void MarkdownHighlighter::highlightBlock(const QString& text)
{
  QTextCharFormat marker;
  marker.setForeground(QColor(QStringLiteral("#607d8b")));
  marker.setFontWeight(QFont::DemiBold);
  QTextCharFormat heading = marker;
  heading.setForeground(QColor(QStringLiteral("#3949ab")));
  QTextCharFormat link;
  link.setForeground(QColor(QStringLiteral("#1565c0")));
  link.setFontUnderline(true);
  QTextCharFormat code;
  code.setForeground(QColor(QStringLiteral("#6a1b9a")));
  QTextCharFormat extension;
  extension.setForeground(QColor(QStringLiteral("#ad1457")));

  const auto apply = [this, &text](const QRegularExpression& expression,
                                    const QTextCharFormat& format) {
    auto match = expression.globalMatch(text);
    while (match.hasNext()) {
      const auto found = match.next();
      setFormat(found.capturedStart(), found.capturedLength(), format);
    }
  };
  apply(QRegularExpression(QStringLiteral("^#{1,6}\\s+.*$")), heading);
  apply(QRegularExpression(QStringLiteral("(`+)[^`]+\\1")), code);
  apply(QRegularExpression(QStringLiteral("!?\\[[^\\]]+\\]\\([^\\)]+\\)")), link);
  apply(QRegularExpression(QStringLiteral("(\\*\\*|__|~~|\\*|_)(?=\\S)")), marker);
  apply(QRegularExpression(QStringLiteral("parchmint:[a-z-]+|style-id=|^:::+")), extension);
}
