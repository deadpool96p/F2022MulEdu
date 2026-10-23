from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import ghidra.app.decompiler
import ghidra.app.plugin
import ghidra.app.services
import ghidra.framework.options
import ghidra.framework.plugintool
import ghidra.program.model.address
import ghidra.program.model.data
import ghidra.program.model.listing
import ghidra.util.task
import java.lang # type: ignore
import java.util # type: ignore


class ComparePinningsTask(ghidra.util.task.Task):

    @typing.type_check_only
    class PinningMatch(java.lang.Record):

        class_: typing.ClassVar[java.lang.Class]

        def equals(self, o: java.lang.Object) -> bool:
            ...

        def hashCode(self) -> int:
            ...

        def sourceOffset(self) -> int:
            ...

        def targetOffset(self) -> int:
            ...

        def weight(self) -> float:
            ...


    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, plugin: StructureRecoveryPlugin) -> None:
        ...

    def getDecompilerResults(self, ifc: ghidra.app.decompiler.DecompInterface, func: ghidra.program.model.listing.Function, monitor: ghidra.util.task.TaskMonitor) -> ghidra.app.decompiler.DecompileResults:
        ...


class StructureRecoveryPlugin(ghidra.app.plugin.ProgramPlugin, ghidra.framework.options.OptionsChangeListener):
    """
    Plugin for recovering structure layouts across programs.
    """

    class FunctionMatchOption(java.lang.Enum[StructureRecoveryPlugin.FunctionMatchOption]):

        class_: typing.ClassVar[java.lang.Class]
        SOLO_MATCHES: typing.Final[StructureRecoveryPlugin.FunctionMatchOption]
        SOLO_OR_MIN_CONFIDENCE: typing.Final[StructureRecoveryPlugin.FunctionMatchOption]
        MAX_WEIGHT: typing.Final[StructureRecoveryPlugin.FunctionMatchOption]

        @staticmethod
        def valueOf(name: typing.Union[java.lang.String, str]) -> StructureRecoveryPlugin.FunctionMatchOption:
            ...

        @staticmethod
        def values() -> jpype.JArray[StructureRecoveryPlugin.FunctionMatchOption]:
            ...


    class_: typing.ClassVar[java.lang.Class]
    HELP_LOCATION: typing.Final = "StructureRecovery"

    def __init__(self, tool: ghidra.framework.plugintool.PluginTool) -> None:
        ...

    def getAddressesByField(self) -> java.util.Map[java.lang.String, ghidra.program.model.address.AddressSet]:
        ...

    def getConsole(self) -> ghidra.app.services.ConsoleService:
        ...

    def getCurrentFunction(self) -> ghidra.program.model.listing.Function:
        ...

    def getDefaultStructureName(self) -> str:
        ...

    def getExcludedOffsets(self) -> java.util.Set[java.lang.Long]:
        ...

    def getFieldNames(self) -> java.util.Map[java.lang.Long, java.lang.String]:
        ...

    def getFunctionMap(self) -> java.util.Map[ghidra.program.model.listing.Function, java.util.Set[ghidra.program.model.listing.Function]]:
        ...

    def getFunctionsByField(self) -> java.util.Map[java.lang.String, java.util.Set[ghidra.program.model.listing.Function]]:
        ...

    def getFunctionsToMatch(self) -> java.util.Set[ghidra.program.model.listing.Function]:
        ...

    def getMatchConfidenceLowerBound(self) -> float:
        ...

    def getMatchSimilarityLowerBound(self) -> float:
        ...

    def getMatchSimilarityUpperBound(self) -> float:
        ...

    def getMaxOffset(self) -> int:
        ...

    def getMinConfidence(self) -> float:
        ...

    def getOffsets(self) -> java.util.Map[java.lang.String, java.lang.Long]:
        ...

    def getOptionFnMatchBookmarks(self) -> bool:
        ...

    def getOptionSoloMatchesOnly(self) -> StructureRecoveryPlugin.FunctionMatchOption:
        ...

    def getOptionStructUseBookmarks(self) -> bool:
        ...

    def getSelfSiginificanceBound(self) -> float:
        ...

    def getTargetDataType(self) -> ghidra.program.model.data.Structure:
        ...

    def getTargetProgram(self) -> ghidra.program.model.listing.Program:
        ...

    def loadTargetProgram(self) -> ghidra.program.model.listing.Program:
        ...

    def setAddressesByField(self, toAddressSet: collections.abc.Mapping) -> None:
        ...

    def setFunctionMap(self, map: collections.abc.Mapping) -> None:
        ...

    def setFunctionsByField(self, functionsByField: collections.abc.Mapping) -> None:
        ...

    def setFunctionsToMatch(self, srcFunctionsToMatch: java.util.Set[ghidra.program.model.listing.Function]) -> None:
        ...

    @property
    def optionSoloMatchesOnly(self) -> StructureRecoveryPlugin.FunctionMatchOption:
        ...

    @property
    def console(self) -> ghidra.app.services.ConsoleService:
        ...

    @property
    def defaultStructureName(self) -> java.lang.String:
        ...

    @property
    def selfSiginificanceBound(self) -> jpype.JDouble:
        ...

    @property
    def fieldNames(self) -> java.util.Map[java.lang.Long, java.lang.String]:
        ...

    @property
    def targetDataType(self) -> ghidra.program.model.data.Structure:
        ...

    @property
    def maxOffset(self) -> jpype.JLong:
        ...

    @property
    def excludedOffsets(self) -> java.util.Set[java.lang.Long]:
        ...

    @property
    def optionStructUseBookmarks(self) -> jpype.JBoolean:
        ...

    @property
    def addressesByField(self) -> java.util.Map[java.lang.String, ghidra.program.model.address.AddressSet]:
        ...

    @addressesByField.setter
    def addressesByField(self, value: java.util.Map[java.lang.String, ghidra.program.model.address.AddressSet]):
        ...

    @property
    def currentFunction(self) -> ghidra.program.model.listing.Function:
        ...

    @property
    def targetProgram(self) -> ghidra.program.model.listing.Program:
        ...

    @property
    def optionFnMatchBookmarks(self) -> jpype.JBoolean:
        ...

    @property
    def matchSimilarityUpperBound(self) -> jpype.JDouble:
        ...

    @property
    def offsets(self) -> java.util.Map[java.lang.String, java.lang.Long]:
        ...

    @property
    def minConfidence(self) -> jpype.JDouble:
        ...

    @property
    def matchSimilarityLowerBound(self) -> jpype.JDouble:
        ...

    @property
    def functionsByField(self) -> java.util.Map[java.lang.String, java.util.Set[ghidra.program.model.listing.Function]]:
        ...

    @functionsByField.setter
    def functionsByField(self, value: java.util.Map[java.lang.String, java.util.Set[ghidra.program.model.listing.Function]]):
        ...

    @property
    def functionsToMatch(self) -> java.util.Set[ghidra.program.model.listing.Function]:
        ...

    @functionsToMatch.setter
    def functionsToMatch(self, value: java.util.Set[ghidra.program.model.listing.Function]):
        ...

    @property
    def matchConfidenceLowerBound(self) -> jpype.JDouble:
        ...

    @property
    def functionMap(self) -> java.util.Map[ghidra.program.model.listing.Function, java.util.Set[ghidra.program.model.listing.Function]]:
        ...

    @functionMap.setter
    def functionMap(self, value: java.util.Map[ghidra.program.model.listing.Function, java.util.Set[ghidra.program.model.listing.Function]]):
        ...


class MatchFunctionsTask(ghidra.util.task.Task):

    class LocalBSimMatch(java.lang.Record):

        class_: typing.ClassVar[java.lang.Class]

        def __init__(self, sourceFunc: ghidra.program.model.listing.Function, targetFunc: ghidra.program.model.listing.Function, sim: typing.Union[jpype.JDouble, float], significance: typing.Union[jpype.JDouble, float]) -> None:
            ...

        def equals(self, o: java.lang.Object) -> bool:
            ...

        def hashCode(self) -> int:
            ...

        def significance(self) -> float:
            ...

        def sim(self) -> float:
            ...

        def sourceFunc(self) -> ghidra.program.model.listing.Function:
            ...

        def targetFunc(self) -> ghidra.program.model.listing.Function:
            ...

        def toString(self) -> str:
            ...


    class_: typing.ClassVar[java.lang.Class]
    TAG: typing.Final = "Function Match"

    def __init__(self, plugin: StructureRecoveryPlugin, target: ghidra.program.model.listing.Program) -> None:
        ...


class RetrieveUsesTask(ghidra.util.task.Task):

    class_: typing.ClassVar[java.lang.Class]
    TAG: typing.Final = "Structure Use"

    def __init__(self, plugin: StructureRecoveryPlugin) -> None:
        ...



__all__ = ["ComparePinningsTask", "StructureRecoveryPlugin", "MatchFunctionsTask", "RetrieveUsesTask"]
