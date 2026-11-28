//! Built-in sample documents and templates (original text, generated in code).

use std::sync::Arc;
use wordcraft_doc::para::InlineObject;
use wordcraft_doc::props::{Align, Border, BorderStyle, Borders, CharProps, NumRef, Rgb, TabAlign, TabLeader, TabStop, TextColor};
use wordcraft_doc::{Block, Document, ListKind, Paragraph, PartKind, Table, para_block};

/// Where the samples' "community board" links: ArtCraft's site in WordCraft's own builds. An
/// edition, built with `WORDCRAFT_DEFAULT_LANGUAGE` (the variable the interface's
/// `links_to_artcraft` follows), is a modified version, and `docs/brand/LICENSE-brand.txt` keeps
/// the links to the ArtCraft community for WordCraft's own builds: its samples link a domain kept
/// for examples instead.
const COMMUNITY_LINK: &str = if option_env!("WORDCRAFT_DEFAULT_LANGUAGE").is_none() { "https://getartcraft.com/" } else { "https://example.org/" };

fn para(text: &str) -> Paragraph {
    Paragraph::with_text(text, CharProps::default())
}

fn styled(text: &str, style: &str) -> Paragraph {
    para(text).styled(style)
}

/// Append text with formatting to a paragraph.
fn add(p: &mut Paragraph, text: &str, props: CharProps) {
    let at = p.len();
    let _ = p.insert_text(at, text, &props);
}

fn bold() -> CharProps {
    CharProps { bold: Some(true), ..Default::default() }
}
fn italic() -> CharProps {
    CharProps { italic: Some(true), ..Default::default() }
}
/// Bold Hebrew text: complex-script text takes its own bold (`w:bCs`), so both are set, as Bold
/// sets them.
fn bold_cs() -> CharProps {
    CharProps { bold: Some(true), bold_cs: Some(true), ..Default::default() }
}
/// Italic Hebrew text (`w:iCs` beside `w:i`).
fn italic_cs() -> CharProps {
    CharProps { italic: Some(true), italic_cs: Some(true), ..Default::default() }
}

/// The showcase document: a short studio handbook with headings, lists, a table, a quote,
/// a footer with page numbers.
pub fn sample_document() -> Document {
    let mut d = Document::new();
    d.core.title = "The Open Studio Handbook".into();
    d.core.creator = "WordCraft".into();
    let mut b: Vec<Block> = Vec::new();
    b.push(Block::Para(styled("The Open Studio Handbook", "Title")));
    b.push(Block::Para(styled("How a small community of makers shares space, tools and time", "Subtitle")));
    let mut intro = para("Every Thursday evening the old print shop on Alder Street fills with people who make things. ");
    add(&mut intro, "Painters", bold());
    add(&mut intro, " set up by the north windows, ", CharProps::default());
    add(&mut intro, "letterpress", italic());
    add(
        &mut intro,
        " regulars claim the back room, and somebody always brings bread. This handbook collects what we have learned about running a shared studio: the routines that keep it pleasant, the tools we pool, and the small agreements that let thirty strangers become a crew.",
        CharProps::default(),
    );
    b.push(Block::Para(intro));
    b.push(Block::Para(styled("Why share a studio?", "Heading1")));
    b.push(Block::Para(para("Rent is the obvious reason, but it is not the best one. A shared room puts your work next to work you would never have seen otherwise. A ceramicist explains glaze chemistry to a comic artist; a photographer borrows a weaver's loom for a still life. The cross-talk is the point.")));
    let mut quote = styled("A studio is a place where unfinished things are welcome.", "Quote");
    quote.props.space_before = Some(12.0);
    b.push(Block::Para(quote));
    b.push(Block::Para(styled("What we agree on", "Heading2")));
    let mut num = (*d.numbering).clone();
    let bullets = num.add_list(ListKind::Bullet);
    let steps = num.add_list(ListKind::Numbered);
    d.numbering = Arc::new(num);
    for (lead, rest) in [
        ("Clean as you go. ", "Leave a table the way you would like to find it."),
        ("Label what is yours. ", "Unlabelled supplies on the commons shelf are fair game."),
        ("Ask before you borrow. ", "Even from the shelf, a quick note helps everyone keep track."),
        ("Quiet hours ", "run from 9 to 11 on weekday mornings."),
    ] {
        let mut p = Paragraph::new().styled("ListParagraph");
        add(&mut p, lead, bold());
        add(&mut p, rest, CharProps::default());
        p.props.numbering = Some(NumRef { num: bullets, level: 0 });
        b.push(Block::Para(p));
    }
    b.push(Block::Para(styled("Shared equipment", "Heading2")));
    b.push(Block::Para(para("The table below lists what the membership owns together and who keeps each item in working order.")));
    let rows = [
        ["Equipment", "Location", "Steward", "Booking"],
        ["Etching press", "Back room", "Mara O.", "Sign-up sheet"],
        ["Kiln (cone 10)", "Yard shed", "Theo P.", "48 h notice"],
        ["Large-format printer", "Office", "Ines R.", "Calendar"],
        ["Light table", "North windows", "Anyone", "First come"],
    ];
    let mut t = Table::new(rows.len(), 4, 468.0);
    t.props.style = Some("GridTable4AccentBlue".into());
    for (r, row) in rows.iter().enumerate() {
        for (c, txt) in row.iter().enumerate() {
            if let Some(cell) = t.rows.get_mut(r).and_then(|x| x.cells.get_mut(c)) {
                cell.blocks = vec![para_block(para(txt))];
            }
        }
    }
    if let Some(r) = t.rows.first_mut() {
        r.props.header = true;
    }
    b.push(Block::Table(t));
    b.push(Block::Para(styled("Opening the studio", "Heading2")));
    for s in [
        "Switch on the main breaker by the door.",
        "Open both skylights unless it is raining.",
        "Start the kettle. This step is not optional.",
        "Check the booking sheet for the kiln and press.",
    ] {
        let mut p = para(s).styled("ListParagraph");
        p.props.numbering = Some(NumRef { num: steps, level: 0 });
        b.push(Block::Para(p));
    }
    b.push(Block::Para(styled("Membership", "Heading1")));
    let mut m = para("Membership is ");
    add(&mut m, "pay what you can", CharProps { bold: Some(true), color: Some(TextColor::Rgb(Rgb(0x15, 0x60, 0x82))), ..Default::default() });
    add(
        &mut m,
        ", reviewed every season. New members shadow a regular for their first two visits, and everyone takes one cleaning shift a month. If you have never made anything before, you are exactly who we hoped would show up.",
        CharProps::default(),
    );
    b.push(Block::Para(m));
    let mut h = para("Questions go to the front desk, or to the notice board by the sink. ");
    add(&mut h, "Visit the community board", CharProps { style: Some("Hyperlink".into()), link: Some(COMMUNITY_LINK.into()), ..Default::default() });
    add(&mut h, " for events.", CharProps::default());
    b.push(Block::Para(h));
    d.body = b.into_iter().map(std::sync::Arc::new).collect();
    // Footer: centred page number.
    let mut f = Paragraph::new().styled("Footer");
    f.props.align = Some(Align::Center);
    let _ = f.insert_object(
        0,
        InlineObject::Field { instr: "PAGE".into(), result: "1".into(), locked: false, dirty: false, code: false, form: String::new() },
        &CharProps::default(),
    );
    let fid = d.add_part(PartKind::Footer, vec![para_block(f)]);
    d.last_section.footers.default = Some(fid);
    let mut hd = styled("The Open Studio Handbook\t\tSpring edition", "Header");
    hd.props.borders = Some(Borders {
        bottom: Some(Border { style: BorderStyle::Single, width: 0.5, color: Some(Rgb(0xBF, 0xBF, 0xBF)), space: 4.0 }),
        ..Default::default()
    });
    let hid = d.add_part(PartKind::Header, vec![para_block(hd)]);
    d.last_section.headers.default = Some(hid);
    d
}

/// A block-style letter.
pub fn letter() -> Document {
    let mut d = Document::new();
    let lines: Vec<Paragraph> = vec![
        styled("Your Name", "Title"),
        para("123 Your Street · City, ST 00000 · you@example.com"),
        para(""),
        para(&crate::cmd::insert::format_date("MMMM d, yyyy")),
        para(""),
        para("Recipient Name"),
        para("Title, Company"),
        para("Street Address"),
        para(""),
        para("Dear Recipient,"),
        para(
            "Start with a sentence that says why you are writing. Keep paragraphs short and specific, and close with what you would like to happen next.",
        ),
        para("Thank you for your time."),
        para("Sincerely,"),
        para(""),
        para("Your Name"),
    ];
    d.body = lines.into_iter().map(para_block).collect();
    d
}

/// A one-page résumé.
pub fn resume() -> Document {
    let mut d = Document::new();
    let mut b: Vec<Block> =
        vec![Block::Para(styled("Alex Rivera", "Title")), Block::Para(para("Illustrator · alex@example.com · portfolio.example.com"))];
    for (head, items) in [
        ("Experience", vec![("Lead Illustrator, Harbor Books", "2021 – present"), ("Freelance Illustrator", "2016 – 2021")]),
        ("Education", vec![("BFA Illustration, Coastal College of Art", "2016")]),
        ("Skills", vec![("Ink, gouache, digital painting, book layout, lettering", "")]),
    ] {
        b.push(Block::Para(styled(head, "Heading1")));
        for (what, when) in items {
            let mut p = para(what);
            p.props.tabs = Some(vec![TabStop { pos: 468.0, align: TabAlign::Right, leader: TabLeader::None }]);
            if !when.is_empty() {
                add(&mut p, &format!("\t{when}"), italic());
            }
            b.push(Block::Para(p));
        }
    }
    d.body = b.into_iter().map(std::sync::Arc::new).collect();
    d
}

/// A report with a cover heading and sections.
pub fn report() -> Document {
    let mut d = Document::new();
    let b: Vec<Block> = vec![
        Block::Para(styled("Report Title", "Title")),
        Block::Para(styled("Subtitle or date", "Subtitle")),
        Block::Para(styled("Summary", "Heading1")),
        Block::Para(para("Summarise the findings in two or three sentences.")),
        Block::Para(styled("Background", "Heading1")),
        Block::Para(para("Describe the context and the question the report answers.")),
        Block::Para(styled("Findings", "Heading1")),
        Block::Para(styled("First finding", "Heading2")),
        Block::Para(para("Explain the evidence.")),
        Block::Para(styled("Recommendations", "Heading1")),
        Block::Para(para("List what should happen next.")),
    ];
    d.body = b.into_iter().map(std::sync::Arc::new).collect();
    d
}

/// The built-in template `id` (`sample`, `letter`, `resume` or `report`), in Hebrew and right to
/// left when `hebrew` (`file.new` with `direction: "rtl"`, as File › New asks for them in a Hebrew
/// interface), else in English; `None` for any other id.
pub fn template(id: &str, hebrew: bool) -> Option<Document> {
    let english = match id {
        "sample" => sample_document,
        "letter" => letter,
        "resume" => resume,
        "report" => report,
        _ => return None,
    };
    if !hebrew {
        return Some(english());
    }
    let mut d = match id {
        "sample" => hebrew_sample_document(),
        "letter" => hebrew_letter(),
        "resume" => hebrew_resume(),
        _ => hebrew_report(),
    };
    crate::cmd::file::set_rtl_defaults(&mut d, true);
    Some(d)
}

/// The showcase document in Hebrew: [`sample_document`]'s handbook, written for Hebrew readers.
fn hebrew_sample_document() -> Document {
    let mut d = Document::new();
    d.core.title = "מדריך הסטודיו הפתוח".into();
    d.core.creator = "WordCraft".into();
    let mut b: Vec<Block> = Vec::new();
    b.push(Block::Para(styled("מדריך הסטודיו הפתוח", "Title")));
    b.push(Block::Para(styled("איך קהילה קטנה של יוצרים חולקת מקום, כלים וזמן", "Subtitle")));
    let mut intro = para("בכל יום חמישי בערב מתמלא בית הדפוס הישן ברחוב האלון באנשים שיוצרים דברים. ");
    add(&mut intro, "ציירים", bold_cs());
    add(&mut intro, " מתמקמים ליד החלונות הצפוניים, אנשי ", CharProps::default());
    add(&mut intro, "דפוס הבלט", italic_cs());
    add(
        &mut intro,
        " תופסים את החדר האחורי, ומישהו תמיד מביא לחם. המדריך הזה מרכז את מה שלמדנו על ניהול סטודיו משותף: ההרגלים ששומרים על אווירה נעימה, הכלים שאנחנו חולקים, וההסכמות הקטנות שהופכות שלושים זרים לצוות אחד.",
        CharProps::default(),
    );
    b.push(Block::Para(intro));
    b.push(Block::Para(styled("למה לחלוק סטודיו?", "Heading1")));
    b.push(Block::Para(para("שכר הדירה הוא הסיבה המובנת מאליה, אבל לא הסיבה הטובה ביותר. חדר משותף מציב את העבודה שלכם ליד עבודות שלא הייתם רואים אחרת. קדרית מסבירה לאמן קומיקס את הכימיה של הזיגוג; צלם שואל נול מאורגת כדי לצלם טבע דומם. השיחה בין התחומים היא העיקר.")));
    let mut quote = styled("סטודיו הוא מקום שבו דברים לא גמורים מתקבלים בברכה.", "Quote");
    quote.props.space_before = Some(12.0);
    b.push(Block::Para(quote));
    b.push(Block::Para(styled("על מה הסכמנו", "Heading2")));
    let mut num = (*d.numbering).clone();
    let bullets = num.add_list(ListKind::Bullet);
    let steps = num.add_list(ListKind::Numbered);
    d.numbering = Arc::new(num);
    for (lead, rest) in [
        ("מנקים תוך כדי עבודה. ", "השאירו את השולחן כפי שהייתם רוצים למצוא אותו."),
        ("מסמנים את מה ששלכם. ", "חומרים בלי שם על המדף המשותף פתוחים לכולם."),
        ("מבקשים לפני שלוקחים. ", "גם כשלוקחים מהמדף, פתק קצר עוזר לכולם לדעת מה איפה."),
        ("שעות השקט ", "הן בין 9 ל-11 בבוקר בימי חול."),
    ] {
        let mut p = Paragraph::new().styled("ListParagraph");
        add(&mut p, lead, bold_cs());
        add(&mut p, rest, CharProps::default());
        p.props.numbering = Some(NumRef { num: bullets, level: 0 });
        b.push(Block::Para(p));
    }
    b.push(Block::Para(styled("ציוד משותף", "Heading2")));
    b.push(Block::Para(para("הטבלה שלהלן מפרטת את הציוד שבבעלות משותפת של החברים, ומי דואג שכל פריט יישאר תקין.")));
    let rows = [
        ["ציוד", "מיקום", "אחראי", "הזמנה"],
        ["מכבש תחריט", "החדר האחורי", "מאיה א.", "דף הרשמה"],
        ["תנור שריפה (קון 10)", "המחסן בחצר", "תום פ.", "48 שעות מראש"],
        ["מדפסת פורמט רחב", "המשרד", "ענבל ר.", "יומן"],
        ["שולחן אור", "החלונות הצפוניים", "כולם", "כל הקודם זוכה"],
    ];
    let mut t = Table::new(rows.len(), 4, 468.0);
    t.props.style = Some("GridTable4AccentBlue".into());
    // A right-to-left table, as one inserted into Hebrew text is: its first column at the right.
    t.props.bidi_visual = true;
    for (r, row) in rows.iter().enumerate() {
        for (c, txt) in row.iter().enumerate() {
            if let Some(cell) = t.rows.get_mut(r).and_then(|x| x.cells.get_mut(c)) {
                cell.blocks = vec![para_block(para(txt))];
            }
        }
    }
    if let Some(r) = t.rows.first_mut() {
        r.props.header = true;
    }
    b.push(Block::Table(t));
    b.push(Block::Para(styled("פתיחת הסטודיו", "Heading2")));
    for s in [
        "הדליקו את המפסק הראשי ליד הדלת.",
        "פתחו את שני חלונות הגג, אלא אם יורד גשם.",
        "הרתיחו מים בקומקום. על השלב הזה אין מוותרים.",
        "בדקו בדף ההזמנות מי רשום לתנור ולמכבש.",
    ] {
        let mut p = para(s).styled("ListParagraph");
        p.props.numbering = Some(NumRef { num: steps, level: 0 });
        b.push(Block::Para(p));
    }
    b.push(Block::Para(styled("חברות בסטודיו", "Heading1")));
    let mut m = para("דמי החברות הם ");
    add(
        &mut m,
        "לפי היכולת",
        CharProps { bold: Some(true), bold_cs: Some(true), color: Some(TextColor::Rgb(Rgb(0x15, 0x60, 0x82))), ..Default::default() },
    );
    add(
        &mut m,
        ", ונקבעים מחדש בכל עונה. חברים חדשים מצטרפים לאחד הוותיקים בשני הביקורים הראשונים שלהם, וכל אחד לוקח משמרת ניקיון אחת בחודש. אם מעולם לא יצרתם שום דבר, אתם בדיוק האנשים שקיווינו שיגיעו.",
        CharProps::default(),
    );
    b.push(Block::Para(m));
    let mut h = para("שאלות אפשר להפנות לדלפק בכניסה, או ללוח המודעות ליד הכיור. ");
    add(&mut h, "בקרו בלוח הקהילה", CharProps { style: Some("Hyperlink".into()), link: Some(COMMUNITY_LINK.into()), ..Default::default() });
    add(&mut h, " כדי לשמוע על אירועים.", CharProps::default());
    b.push(Block::Para(h));
    d.body = b.into_iter().map(std::sync::Arc::new).collect();
    let mut f = Paragraph::new().styled("Footer");
    f.props.align = Some(Align::Center);
    let _ = f.insert_object(
        0,
        InlineObject::Field { instr: "PAGE".into(), result: "1".into(), locked: false, dirty: false, code: false, form: String::new() },
        &CharProps::default(),
    );
    let fid = d.add_part(PartKind::Footer, vec![para_block(f)]);
    d.last_section.footers.default = Some(fid);
    let mut hd = styled("מדריך הסטודיו הפתוח\t\tמהדורת אביב", "Header");
    hd.props.borders = Some(Borders {
        bottom: Some(Border { style: BorderStyle::Single, width: 0.5, color: Some(Rgb(0xBF, 0xBF, 0xBF)), space: 4.0 }),
        ..Default::default()
    });
    let hid = d.add_part(PartKind::Header, vec![para_block(hd)]);
    d.last_section.headers.default = Some(hid);
    d
}

/// [`letter`] in Hebrew: the recipient after "לכבוד" and a subject line (הנדון), as Hebrew letters
/// have them, dated in Hebrew.
fn hebrew_letter() -> Document {
    let mut d = Document::new();
    let date = crate::cmd::words::long_date(Some("he-IL"));
    let lines: Vec<Paragraph> = vec![
        styled("השם שלכם", "Title"),
        para("הכתובת שלכם · עיר, מיקוד · you@example.com"),
        para(""),
        para(&date),
        para(""),
        para("לכבוד"),
        para("שם הנמען"),
        para("תפקיד, חברה"),
        para("כתובת"),
        para(""),
        para("הנדון: נושא המכתב"),
        para(""),
        para("שלום רב,"),
        para("פתחו במשפט שמסביר מדוע אתם כותבים. כתבו פסקאות קצרות וענייניות, וסיימו במה שאתם מבקשים שיקרה בהמשך."),
        para("תודה על תשומת לבכם."),
        para("בברכה,"),
        para(""),
        para("השם שלכם"),
    ];
    d.body = lines.into_iter().map(para_block).collect();
    d
}

/// [`resume`] in Hebrew.
fn hebrew_resume() -> Document {
    let mut d = Document::new();
    // The dates at the far end of the line: an end-aligned tab at the text's width.
    let width = d.last_section.text_width();
    let mut b: Vec<Block> = vec![Block::Para(styled("נועה לוי", "Title")), Block::Para(para("מאיירת · noa@example.com · portfolio.example.com"))];
    for (head, items) in [
        ("ניסיון תעסוקתי", vec![("מאיירת ראשית, הוצאת ספרי הנמל", "2021 – היום"), ("מאיירת עצמאית", "2016 – 2021")]),
        ("השכלה", vec![("תואר ראשון באיור, מכללת החוף לאמנות", "2016")]),
        ("כישורים", vec![("דיו, גואש, ציור דיגיטלי, עימוד ספרים, עיצוב אותיות", "")]),
    ] {
        b.push(Block::Para(styled(head, "Heading1")));
        for (what, when) in items {
            let mut p = para(what);
            p.props.tabs = Some(vec![TabStop { pos: width, align: TabAlign::Right, leader: TabLeader::None }]);
            if !when.is_empty() {
                add(&mut p, &format!("\t{when}"), italic_cs());
            }
            b.push(Block::Para(p));
        }
    }
    d.body = b.into_iter().map(std::sync::Arc::new).collect();
    d
}

/// [`report`] in Hebrew.
fn hebrew_report() -> Document {
    let mut d = Document::new();
    let b: Vec<Block> = vec![
        Block::Para(styled("כותרת הדוח", "Title")),
        Block::Para(styled("כותרת משנה או תאריך", "Subtitle")),
        Block::Para(styled("תקציר", "Heading1")),
        Block::Para(para("סכמו את הממצאים בשניים או שלושה משפטים.")),
        Block::Para(styled("רקע", "Heading1")),
        Block::Para(para("תארו את ההקשר ואת השאלה שהדוח עונה עליה.")),
        Block::Para(styled("ממצאים", "Heading1")),
        Block::Para(styled("ממצא ראשון", "Heading2")),
        Block::Para(para("הציגו את הנתונים שעליהם הממצא נשען.")),
        Block::Para(styled("המלצות", "Heading1")),
        Block::Para(para("פרטו מה צריך לקרות בהמשך.")),
    ];
    d.body = b.into_iter().map(std::sync::Arc::new).collect();
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_valid() {
        assert_eq!(sample_document().word_count(), 361);
        for d in [sample_document(), letter(), resume(), report()] {
            assert!(d.word_count() > 5);
            let l = wordcraft_layout::layout(&d, &mut wordcraft_layout::LayoutCache::new(), &Default::default());
            assert!(!l.pages.is_empty());
        }
    }

    /// Each template in Hebrew is a right-to-left Hebrew document with the English one's
    /// structure: the same styles in the same order, and a right-to-left section.
    #[test]
    fn hebrew_templates_are_right_to_left_hebrew_documents_shaped_like_the_english_ones() {
        let styles = |d: &Document| d.body.iter().map(|b| b.as_para().map(|p| p.props.style.clone())).collect::<Vec<_>>();
        for id in ["sample", "letter", "resume", "report"] {
            let (Some(en), Some(he)) = (template(id, false), template(id, true)) else { panic!("{id} is a template") };
            assert!(he.styles.resolve_para(&Default::default()).bidi && he.last_section.rtl, "{id} reads right to left");
            assert!(!en.styles.resolve_para(&Default::default()).bidi && !en.last_section.rtl, "{id} in English stays left to right");
            let text = he.plain_text(wordcraft_doc::StoryRef::Body);
            let latin_words = text.split_whitespace().filter(|w| w.chars().any(|c| c.is_ascii_alphabetic()) && !w.contains(['@', '.'])).count();
            assert!(wordcraft_doc::bidi::has_rtl(&text) && latin_words == 0, "{id} is written in Hebrew: {text}");
            if id != "letter" {
                assert_eq!(styles(&he), styles(&en), "{id} has the English template's structure");
            }
            let l = wordcraft_layout::layout(&he, &mut wordcraft_layout::LayoutCache::new(), &Default::default());
            assert!(!l.pages.is_empty());
        }
        assert!(template("blank", true).is_none() && template("nope", false).is_none());
    }
}
