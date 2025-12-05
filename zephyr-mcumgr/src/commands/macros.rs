/// Implements the [`McuMgrCommand`] trait for a request/response pair.
///
/// # Parameters
/// - `$request`: The request type implementing the command
/// - `$response`: The response type for this command
/// - `$iswrite`: Boolean literal indicating if this is a write operation
/// - `$groupid`: The MCUmgr group
/// - `$commandid`: The MCUmgr command ID (u8)
macro_rules! impl_mcumgr_command {
    (@direction read) => {false};
    (@direction write) => {true};
    (($direction:tt, $groupid:ident, $commandid:literal): $request:ty => $response:ty) => {
        impl McuMgrCommand for $request {
            type Payload = Self;
            type Response = $response;
            fn is_write_operation(&self) -> bool {
                impl_mcumgr_command!(@direction $direction)
            }
            fn group_id(&self) -> u16 {
                $crate::MCUmgrGroup::$groupid as u16
            }
            fn command_id(&self) -> u8 {
                $commandid
            }
            fn data(&self) -> &Self {
                self
            }
        }
    };
}

pub(super) use impl_mcumgr_command;

#[cfg(test)]
macro_rules! command_encode_decode_test {
    (@is_write 0) => {false};
    (@is_write 2) => {true};
    ($name:ident, ($op:tt, $group_id:literal, $command_id:literal), $request:expr, $encoded_req:expr ,$encoded_res:expr, $response:expr $(,)?) => {
        #[test]
        fn $name() {
            use $crate::commands::McuMgrCommand;

            let expected_is_write = command_encode_decode_test!(@is_write $op);
            assert_eq!($request.is_write_operation(), expected_is_write);
            assert_eq!($request.group_id(), $group_id);
            assert_eq!($request.command_id(), $command_id);

            let mut encoded_request = vec![];
            ::ciborium::into_writer(&$request.data(), &mut encoded_request).unwrap();

            let mut expected_encoded_request = vec![];
            ::ciborium::into_writer(&$encoded_req.unwrap(), &mut expected_encoded_request).unwrap();

            assert_eq!(
                encoded_request.iter().map(|x|format!("{:02x}", x)).collect::<String>(),
                expected_encoded_request.iter().map(|x|format!("{:02x}", x)).collect::<String>(),
                "encoding mismatch"
            );

            let mut encoded_response = vec![];
            ::ciborium::into_writer(&$encoded_res.unwrap(), &mut encoded_response).unwrap();

            // Compile time type check
            fn types_match<T: $crate::commands::McuMgrCommand>(
                _req: T,
                _res: <T as $crate::commands::McuMgrCommand>::Response,
            ) {
            }
            types_match($request, $response);

            let response = ::ciborium::from_reader(encoded_response.as_slice()).unwrap();
            let expected_response = $response;

            assert_eq!(response, expected_response, "decoding mismatch");

            // As a type hint for ciborium
            types_match($request, response);
        }
    };
}

#[cfg(test)]
pub(super) use command_encode_decode_test;

macro_rules! impl_serialize_as_empty_map {
    ($type:ty) => {
        impl ::serde::Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: ::serde::Serializer,
            {
                const _: () = [()][size_of::<$type>()]; // Fails if type is not a unit struct

                use ::serde::ser::SerializeMap;
                let map = serializer.serialize_map(Some(0))?;
                map.end()
            }
        }
    };
}

macro_rules! impl_deserialize_from_empty_map_and_into_unit {
    ($type:ty) => {
        impl From<$type> for () {
            fn from(_: $type) -> () {
                const _: () = [()][size_of::<$type>()]; // Fails if type is not a unit struct
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: ::serde::Deserializer<'de>,
            {
                struct InternalVisitor;

                impl<'de> ::serde::de::Visitor<'de> for InternalVisitor {
                    type Value = $type;

                    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                        write!(f, "an empty map/object (`{{}}`)")
                    }

                    fn visit_map<M>(self, mut map: M) -> Result<$type, M::Error>
                    where
                        M: ::serde::de::MapAccess<'de>,
                    {
                        let mut had_entries = false;

                        while map
                            .next_entry::<::serde::de::IgnoredAny, ::serde::de::IgnoredAny>()?
                            .is_some()
                        {
                            had_entries = true;
                        }

                        // Do not error when there are entries; we also accept non-empty maps for future compatibility.
                        if had_entries {
                            ::log::debug!(
                                "Ignoring unexpected entries in unit-type deserialization"
                            );
                        }

                        Ok(<$type>::default())
                    }
                }

                deserializer.deserialize_map(InternalVisitor)
            }
        }
    };
}

pub(super) use impl_deserialize_from_empty_map_and_into_unit;
pub(super) use impl_serialize_as_empty_map;
