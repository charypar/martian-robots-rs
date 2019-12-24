use std::io::Read;
use std::str;

use combine::error::ParseError;
use combine::parser::byte::{digit, spaces};
use combine::stream::buffered;
use combine::stream::position;
use combine::stream::read;
use combine::stream::Stream;
use combine::{many1, one_of, Parser};

use crate::geo::location::Point;
use crate::geo::orientation::Orientation;
use crate::mission::Mission;
use crate::robot::{Command, Robot};

pub struct MissionPlan<'a, R>
where
    R: Read,
{
    pub mission: Mission,
    stream:
        Box<buffered::Stream<position::Stream<read::Stream<&'a mut R>, position::IndexPositioner>>>,
}

// Parses an X, Y point written as two integers separated by whitespace
fn point<Input>() -> impl Parser<Input, Output = Point>
where
    Input: Stream<Token = u8>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    (many1(digit()), spaces(), many1(digit())).map(|(x, _, y): (Vec<u8>, _, Vec<u8>)| Point {
        x: str::from_utf8(&x).unwrap().parse().unwrap(),
        y: str::from_utf8(&y).unwrap().parse().unwrap(),
    })
}

// Parses an orientation written as a single letter N, E, S or W
fn orientation<Input>() -> impl Parser<Input, Output = Orientation>
where
    Input: Stream<Token = u8>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    one_of("NESW".bytes()).map(|token: u8| match token as char {
        'N' => Orientation::North,
        'E' => Orientation::East,
        'S' => Orientation::South,
        'W' => Orientation::West,
        _ => panic!(),
    })
}

// Parses a contiguous series of commands L, R or F
fn commands<Input>() -> impl Parser<Input, Output = Vec<Command>>
where
    Input: Stream<Token = u8>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    many1(one_of("LRF".bytes())).map(|cmds: Vec<u8>| {
        cmds.iter()
            .map(|cmd| match *cmd as char {
                'L' => Command::Left,
                'R' => Command::Right,
                'F' => Command::Forward,
                _ => panic!(),
            })
            .collect()
    })
}

// Parses a robot definition followed by instructions
fn robot<Input>() -> impl Parser<Input, Output = (Robot, Vec<Command>)>
where
    Input: Stream<Token = u8>,
    Input::Error: ParseError<Input::Token, Input::Range, Input::Position>,
{
    point()
        .skip(spaces())
        .and(orientation())
        .skip(spaces()) // spaces covers new lines
        .and(commands())
        .skip(spaces())
        .map(|((point, orientation), commands)| {
            (
                Robot {
                    position: point,
                    facing: orientation,
                },
                commands,
            )
        })
}

impl<R> MissionPlan<'_, R>
where
    R: Read,
{
    pub fn read(input: &mut R) -> MissionPlan<R> {
        // Should return Result
        let mut stream = buffered::Stream::new(position::Stream::new(read::Stream::new(input)), 1);
        let mission;

        {
            let mut upper_right = point().skip(spaces());
            let point = upper_right.parse(&mut stream);

            mission = match point {
                Ok((point, _)) => Mission::new(point),
                Err(err) => panic!("Err! {}", err),
            };
        } // drop upper_right and therefore release stream

        MissionPlan {
            mission: mission,
            stream: Box::new(stream),
        }
    }
}

impl<R> Iterator for MissionPlan<'_, R>
where
    R: Read,
{
    type Item = (Robot, Vec<Command>);

    fn next(&mut self) -> Option<Self::Item> {
        let stream = self.stream.as_mut();
        let robot = robot().skip(spaces()).parse(stream);

        match robot {
            Ok((robot, _)) => Some(robot),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use combine;
    use combine::stream::position;
    use std::io::Cursor;

    use crate::geo::location::Point;
    use crate::mission::Mission;
    use crate::robot::Command::{Forward as F, Left as L, Right as R};

    #[test]
    fn recognises_a_simple_point() {
        let input = b"3 4";
        let (actual, _) = point().parse(position::Stream::new(&input[..])).unwrap();
        let expected = Point { x: 3, y: 4 };

        assert_eq!(actual, expected)
    }

    #[test]
    fn recognises_a_point_with_extra_whitespace() {
        let input = b"3  \t 4";
        let (actual, _) = point().parse(position::Stream::new(&input[..])).unwrap();
        let expected = Point { x: 3, y: 4 };

        assert_eq!(actual, expected)
    }

    #[test]
    fn does_not_recognise_a_point_with_whitespaces_in_front() {
        let input = b" 3  \t 4";
        let actual = point().parse(position::Stream::new(&input[..]));
        let expected = Err(combine::error::UnexpectedParse::Unexpected);

        assert_eq!(actual, expected);
    }

    #[test]
    fn recognises_commands() {
        let input = b"LRFFLFR";
        let (actual, _) = commands().parse(position::Stream::new(&input[..])).unwrap();
        let expected = vec![L, R, F, F, L, F, R];

        assert_eq!(actual, expected)
    }

    #[test]
    fn recognises_a_robot() {
        let input = b"4  5  W\n\r  LRFFLFR"; // we don't discriminate against whitespace
        let (actual, _) = robot().parse(position::Stream::new(&input[..])).unwrap();
        let expected = (
            Robot {
                position: Point { x: 4, y: 5 },
                facing: Orientation::West,
            },
            vec![L, R, F, F, L, F, R],
        );

        assert_eq!(actual, expected)
    }

    #[test]
    fn reads_basic_mission_plan() {
        let mut input = Cursor::new("31 24\n");

        let actual = MissionPlan::read(&mut input).mission;
        let expected = Mission::new(Point { x: 31, y: 24 });

        assert_eq!(actual, expected)
    }

    #[test]
    fn reads_one_robot() {
        let mut input = Cursor::new("31 24\n1 1 E\nLFLFLFLF");

        let actual = MissionPlan::read(&mut input).next();
        let expected = Some((
            Robot {
                position: Point { x: 1, y: 1 },
                facing: Orientation::East,
            },
            vec![L, F, L, F, L, F, L, F],
        ));

        assert_eq!(actual, expected)
    }

    #[test]
    fn collects_three_robots() {
        let mut input =
            Cursor::new("31 24\n1 1 E\nLFLFLFLF\n\n3 2 N\nFRRFLLFFRRFLL\n\n0 3 W\nLLFFFLFLFL");

        let plan = MissionPlan::read(&mut input);
        let actual: Vec<(Robot, Vec<Command>)> = plan.collect();
        let expected = vec![
            (
                Robot {
                    position: Point { x: 1, y: 1 },
                    facing: Orientation::East,
                },
                vec![L, F, L, F, L, F, L, F],
            ),
            (
                Robot {
                    position: Point { x: 3, y: 2 },
                    facing: Orientation::North,
                },
                vec![F, R, R, F, L, L, F, F, R, R, F, L, L],
            ),
            (
                Robot {
                    position: Point { x: 0, y: 3 },
                    facing: Orientation::West,
                },
                vec![L, L, F, F, F, L, F, L, F, L],
            ),
        ];

        assert_eq!(actual, expected);
    }
}