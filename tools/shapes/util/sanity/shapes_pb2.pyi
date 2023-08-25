from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Iterable as _Iterable, Mapping as _Mapping, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ShapesIteration(_message.Message):
    __slots__ = ["epochs"]
    EPOCHS_FIELD_NUMBER: _ClassVar[int]
    epochs: _containers.RepeatedCompositeFieldContainer[ShapesEpoch]
    def __init__(self, epochs: _Optional[_Iterable[_Union[ShapesEpoch, _Mapping]]] = ...) -> None: ...

class ShapesEpoch(_message.Message):
    __slots__ = ["shapes"]
    SHAPES_FIELD_NUMBER: _ClassVar[int]
    shapes: _containers.RepeatedCompositeFieldContainer[Shape]
    def __init__(self, shapes: _Optional[_Iterable[_Union[Shape, _Mapping]]] = ...) -> None: ...

class Shape(_message.Message):
    __slots__ = ["kind", "object", "offsets"]
    class Kind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = []
        ValArray: _ClassVar[Shape.Kind]
        ObjArray: _ClassVar[Shape.Kind]
        Scalar: _ClassVar[Shape.Kind]
    ValArray: Shape.Kind
    ObjArray: Shape.Kind
    Scalar: Shape.Kind
    KIND_FIELD_NUMBER: _ClassVar[int]
    OBJECT_FIELD_NUMBER: _ClassVar[int]
    OFFSETS_FIELD_NUMBER: _ClassVar[int]
    kind: Shape.Kind
    object: int
    offsets: _containers.RepeatedScalarFieldContainer[int]
    def __init__(self, kind: _Optional[_Union[Shape.Kind, str]] = ..., object: _Optional[int] = ..., offsets: _Optional[_Iterable[int]] = ...) -> None: ...
