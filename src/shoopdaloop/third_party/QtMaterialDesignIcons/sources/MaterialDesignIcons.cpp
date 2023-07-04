#include "MaterialDesignIcons.h"

#include <QFile>

#include <QHash>
#include <QPainter>
#include <QDebug>
#include <QXmlDefaultHandler>
#include <QXmlSimpleReader>
#include <QSvgRenderer>

#include <mutex>


QPixmap material::pixmap(const QString& name, const QSize& size, const QColor& color)
{
	static QHash<QString, QString> svg_paths_;
	static std::once_flag once_;

	std::call_once(once_, [&]
	{
		class SvgGlyphReader : public QXmlDefaultHandler
		{
		public:
			bool startElement(const QString& namespace_uri, const QString& local_name,
							  const QString& qualified_name, const QXmlAttributes& atts) override
			{
				if (local_name != "glyph")
				{
					return true;
				}

				auto glyph_name = atts.value("glyph-name");
				auto path_data = atts.value("d");

				if (glyph_name.isEmpty() || path_data.isEmpty())
				{
					qDebug() << "glyph_name or SVG path data not found in the SVG Font";
					return false;
				}

				svg_paths_.insert(glyph_name, path_data);

				return true;
			}
		};

		SvgGlyphReader glyph_reader;
		QXmlSimpleReader xml_parser;
		xml_parser.setContentHandler(&glyph_reader);

		QFile svg_font(":/materialdesignicons-webfont.svg");
		svg_font.open(QFile::ReadOnly);
		QXmlInputSource xml_source(&svg_font);

		bool parse_ok = xml_parser.parse(&xml_source);
		if (!parse_ok)
		{
			qDebug() << "SVG parsing failed! Something changed in the SVG Font?";
		}
	});

	QPixmap pix(size);
	pix.fill(Qt::transparent);

	const auto svg = [](const QString& path, QColor color)
	{

		// The magic part is to do the proper adjustments according to how the original SVG font is created
		// (i.e. do some offsets, view-box correction, translate & scale, and color fill on top of that)
		return QString(
			R"(<?xml version="1.0" encoding="utf-8"?>)"
			R"(<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">)"
			R"(<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink")"
			R"( version="1.1" baseProfile="full" viewBox="0 -64 512 512" xml:space="preserve">)"
			R"-(<path transform="translate(0, 384), scale(1, -1)" fill="%1" d="%2"/>)-"
			R"(</svg>)")
				.arg(color.name())
				.arg(path).toLocal8Bit();
	};

	QPainter painter(&pix);
	QSvgRenderer svg_renderer;
	svg_renderer.load(svg(svg_paths_[name], color));
	svg_renderer.render(&painter);

	return pix;

}

