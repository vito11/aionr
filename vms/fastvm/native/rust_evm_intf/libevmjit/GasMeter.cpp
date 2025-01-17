#include "GasMeter.h"

#include "preprocessor/llvm_includes_start.h"
#include <llvm/IR/IntrinsicInst.h>
#include "preprocessor/llvm_includes_end.h"

#include "JIT.h"
#include "Ext.h"
#include "RuntimeManager.h"

namespace dev
{
namespace eth
{
namespace jit
{

GasMeter::GasMeter(IRBuilder& _builder, RuntimeManager& _runtimeManager, evm_revision rev, llvm::GlobalVariable* _gasout):
	CompilerHelper(_builder),
	m_runtimeManager(_runtimeManager),
    m_rev(rev),
	m_gasout(_gasout)

{
	llvm::Type* gasCheckArgs[] = {Type::Gas->getPointerTo(), Type::Gas, Type::BytePtr};
    llvm::ConstantInt* constant_1 = llvm::ConstantInt::get(Type::Bool, 1);

	m_gasCheckFunc = llvm::Function::Create(llvm::FunctionType::get(Type::Bool, gasCheckArgs, false), llvm::Function::PrivateLinkage, "gas.check", getModule());
	m_gasCheckFunc->setDoesNotThrow();
	m_gasCheckFunc->addAttribute(1, llvm::Attribute::NoCapture);

	auto checkBB = llvm::BasicBlock::Create(_builder.getContext(), "Check", m_gasCheckFunc);
	auto updateBB = llvm::BasicBlock::Create(_builder.getContext(), "Update", m_gasCheckFunc);
	auto outOfGasBB = llvm::BasicBlock::Create(_builder.getContext(), "OutOfGas", m_gasCheckFunc);

	auto iter = m_gasCheckFunc->arg_begin();
	llvm::Argument* gasPtr = &(*iter++);
	gasPtr->setName("gasPtr");
	llvm::Argument* cost = &(*iter++);
	cost->setName("cost");
	llvm::Argument* jmpBuf = &(*iter);
	jmpBuf->setName("jmpBuf");

	InsertPointGuard guard(m_builder);
	m_builder.SetInsertPoint(checkBB);
	auto gas = m_builder.CreateLoad(gasPtr, "gas");
	auto gasUpdated = m_builder.CreateNSWSub(gas, cost, "gasUpdated");
	auto gasOk = m_builder.CreateICmpSGE(gasUpdated, m_builder.getInt64(0), "gasOk"); // gas >= 0, with gas == 0 we can still do 0 cost instructions
	m_builder.CreateCondBr(gasOk, updateBB, outOfGasBB, Type::expectTrue);

	m_builder.SetInsertPoint(updateBB);
	m_builder.CreateStore(gasUpdated, gasPtr);

	m_builder.CreateRet(m_builder.getInt1(0));

	m_builder.SetInsertPoint(outOfGasBB);
	
	m_builder.CreateStore(m_builder.getInt1(1), m_gasout);
	//m_runtimeManager.abort(jmpBuf);
    //aarch64 not support setjmp/longjmp exception, so we return bool here, and exit on mainFun block 
	m_builder.CreateRet(m_builder.getInt1(1));
	//m_builder.CreateUnreachable();
}

void GasMeter::count(Instruction _inst)
{
	if (!m_checkCall)
	{
		// Create gas check call with mocked block cost at begining of current cost-block
		m_checkCall = m_builder.CreateCall(m_gasCheckFunc, {m_runtimeManager.getGasPtr(), llvm::UndefValue::get(Type::Gas), m_runtimeManager.getJmpBuf()});
	    //m_runtimeManager.myexit(ReturnCode::OutOfGas,m_checkCall);
	}

	m_blockCost += getStepCost(_inst);
}

void GasMeter::count(llvm::Value* _cost, llvm::Value* _jmpBuf, llvm::Value* _gasPtr)
{
	if (_cost->getType() == Type::Word)
	{
		auto gasMax128 = m_builder.CreateZExt(Constant::gasMax, Type::Word);
		auto tooHigh = m_builder.CreateICmpUGT(_cost, gasMax128, "costTooHigh");
		auto cost64 = m_builder.CreateTrunc(_cost, Type::Gas);
		_cost = m_builder.CreateSelect(tooHigh, Constant::gasMax, cost64, "cost");
	}

	assert(_cost->getType() == Type::Gas);
	llvm::CallInst* gas_check = m_builder.CreateCall(m_gasCheckFunc, {_gasPtr ? _gasPtr : m_runtimeManager.getGasPtr(), _cost, _jmpBuf ? _jmpBuf : m_runtimeManager.getJmpBuf()});
	//m_runtimeManager.myexit(ReturnCode::OutOfGas,gas_check);
}
void GasMeter::countExp(llvm::Value* _exponent)
{
	// Additional cost is 1 per significant byte of exponent
	// lz - leading zeros
	// cost = ((128 - lz) + 7) / 8

	// OPT: Can gas update be done in exp algorithm?
	auto ctlz = llvm::Intrinsic::getDeclaration(getModule(), llvm::Intrinsic::ctlz, Type::Word);
	auto lz128 = m_builder.CreateCall(ctlz, {_exponent, m_builder.getInt1(false)});
	auto lz = m_builder.CreateTrunc(lz128, Type::Gas, "lz");
	auto sigBits = m_builder.CreateSub(m_builder.getInt64(128), lz, "sigBits");
	auto sigBytes = m_builder.CreateUDiv(m_builder.CreateAdd(sigBits, m_builder.getInt64(7)), m_builder.getInt64(8));
	auto exponentByteCost = m_rev >= EVM_AION ? 1 : (m_rev >= EVM_SPURIOUS_DRAGON ? 50 : JITSchedule::expByteGas::value);
	count(m_builder.CreateNUWMul(sigBytes, m_builder.getInt64(exponentByteCost)));
}

void GasMeter::countSStore(Ext& _ext, llvm::Value* _index, llvm::Value* _newValue)
{
	auto oldValue = _ext.sload(_index);
	auto oldValueIsZero = m_builder.CreateICmpEQ(oldValue, Constant::get(0), "oldValueIsZero");
	auto newValueIsntZero = m_builder.CreateICmpNE(_newValue, Constant::get(0), "newValueIsntZero");
	auto isInsert = m_builder.CreateAnd(oldValueIsZero, newValueIsntZero, "isInsert");
	assert(JITSchedule::sstoreResetGas::value == JITSchedule::sstoreClearGas::value && "Update SSTORE gas cost");
	auto cost = m_builder.CreateSelect(isInsert, m_builder.getInt64(JITSchedule::sstoreSetGas::value), m_builder.getInt64(m_rev >= EVM_AION ? 8000 : JITSchedule::sstoreResetGas::value), "cost");
	count(cost);
}

void GasMeter::countLogData(llvm::Value* _dataLength)
{
	assert(m_checkCall);
	assert(m_blockCost > 0); // LOGn instruction is already counted
	assert(JITSchedule::logDataGas::value != 1 && "Log data gas cost has changed. Update GasMeter.");
	count(m_builder.CreateNUWMul(_dataLength, Constant::get(m_rev >= EVM_AION ? 20 : JITSchedule::logDataGas::value))); // TODO: Use i64
}

void GasMeter::countSha3Data(llvm::Value* _dataLength)
{
	assert(m_checkCall);
	assert(m_blockCost > 0); // SHA3 instruction is already counted

	// TODO: This round ups to 32 happens in many places
	assert(JITSchedule::sha3WordGas::value != 1 && "SHA3 data cost has changed. Update GasMeter");
	auto dataLength64 = m_builder.CreateTrunc(_dataLength, Type::Gas);
	auto words64 = m_builder.CreateUDiv(m_builder.CreateNUWAdd(dataLength64, m_builder.getInt64(31)), m_builder.getInt64(32));
	auto cost64 = m_builder.CreateNUWMul(m_builder.getInt64(JITSchedule::sha3WordGas::value), words64);
	count(cost64);
}

void GasMeter::giveBack(llvm::Value* _gas)
{
	assert(_gas->getType() == Type::Gas);
	m_runtimeManager.setGas(m_builder.CreateAdd(m_runtimeManager.getGas(), _gas));
}

void GasMeter::commitCostBlock()
{
	// If any uncommited block
	if (m_checkCall)
	{
		if (m_blockCost == 0) // Do not check 0
		{
			m_checkCall->eraseFromParent(); // Remove the gas check call
			m_checkCall = nullptr;
			return;
		}

		m_checkCall->setArgOperand(1, m_builder.getInt64(m_blockCost)); // Update block cost in gas check call
		m_checkCall = nullptr; // End cost-block
		m_blockCost = 0;
	}
	assert(m_blockCost == 0);
}

void GasMeter::countMemory(llvm::Value* _additionalMemoryInWords, llvm::Value* _jmpBuf, llvm::Value* _gasPtr)
{
	assert(JITSchedule::memoryGas::value != 1 && "Memory gas cost has changed. Update GasMeter.");
	count(_additionalMemoryInWords, _jmpBuf, _gasPtr);
}

void GasMeter::countCopy(llvm::Value* _copyWords)
{
	assert(JITSchedule::copyGas::value != 1 && "Copy gas cost has changed. Update GasMeter.");
	count(m_builder.CreateNUWMul(_copyWords, m_builder.getInt64(JITSchedule::copyGas::value)));
}

int64_t GasMeter::getStepCost(Instruction inst) const
{
	switch (inst)
	{
	// Tier 0
	case Instruction::STOP:
	case Instruction::RETURN:
	case Instruction::REVERT:
	case Instruction::SSTORE: // Handle cost of SSTORE separately in GasMeter::countSStore()
		return JITSchedule::stepGas0::value;

	// Tier 1
	case Instruction::ADDRESS:
	case Instruction::ORIGIN:
	case Instruction::CALLER:
	case Instruction::CALLVALUE:
	case Instruction::CALLDATASIZE:
	case Instruction::RETURNDATASIZE:
	case Instruction::CODESIZE:
	case Instruction::GASPRICE:
	case Instruction::COINBASE:
	case Instruction::TIMESTAMP:
	case Instruction::NUMBER:
	case Instruction::DIFFICULTY:
	case Instruction::GASLIMIT:
	case Instruction::POP:
	case Instruction::PC:
	case Instruction::MSIZE:
	case Instruction::GAS:
		return m_rev >= EVM_AION? 1 : JITSchedule::stepGas1::value;

	// Tier 2
	case Instruction::ADD:
	case Instruction::SUB:
	case Instruction::LT:
	case Instruction::GT:
	case Instruction::SLT:
	case Instruction::SGT:
	case Instruction::EQ:
	case Instruction::ISZERO:
	case Instruction::AND:
	case Instruction::OR:
	case Instruction::XOR:
	case Instruction::NOT:
	case Instruction::BYTE:
	case Instruction::CALLDATALOAD:
	case Instruction::CALLDATACOPY:
	case Instruction::RETURNDATACOPY:
	case Instruction::CODECOPY:
	case Instruction::MLOAD:
	case Instruction::MSTORE:
	case Instruction::MSTORE8:
	case Instruction::ANY_PUSH:
	case Instruction::BASE_DUP:
	case Instruction::BASE_SWAP:
	case Instruction::EXT_DUP:
	case Instruction::EXT_SWAP:
		return m_rev >= EVM_AION? 1 : JITSchedule::stepGas2::value;

	// Tier 3
	case Instruction::MUL:
	case Instruction::DIV:
	case Instruction::SDIV:
	case Instruction::MOD:
	case Instruction::SMOD:
	case Instruction::SIGNEXTEND:
		return m_rev >= EVM_AION? 1 : JITSchedule::stepGas3::value;

	// Tier 4
	case Instruction::ADDMOD:
	case Instruction::MULMOD:
	case Instruction::JUMP:
		return m_rev >= EVM_AION? 1 : JITSchedule::stepGas4::value;

	// Tier 5
	case Instruction::EXP:
	case Instruction::JUMPI:
		return m_rev >= EVM_AION? 1 : JITSchedule::stepGas5::value;

	// Tier 6
	case Instruction::BALANCE:
		return m_rev >= EVM_AION ? 1000 : (m_rev >= EVM_TANGERINE_WHISTLE ? 400 : JITSchedule::stepGas6::value);

	case Instruction::EXTCODESIZE:
	case Instruction::EXTCODECOPY:
		return m_rev >= EVM_AION ? 1000 : (m_rev >= EVM_TANGERINE_WHISTLE ? 700 : JITSchedule::stepGas6::value);

	case Instruction::BLOCKHASH:
		return JITSchedule::stepGas6::value;

	case Instruction::SHA3:
		return JITSchedule::sha3Gas::value;

	case Instruction::SLOAD:
		return m_rev >= EVM_AION ? 1000 : (m_rev >= EVM_TANGERINE_WHISTLE ? 200 : JITSchedule::sloadGas::value);

	case Instruction::JUMPDEST:
		return JITSchedule::jumpdestGas::value;

	case Instruction::LOG0:
	case Instruction::LOG1:
	case Instruction::LOG2:
	case Instruction::LOG3:
	case Instruction::LOG4:
	{
		auto numTopics = static_cast<int64_t>(inst) - static_cast<int64_t>(Instruction::LOG0);
		return (m_rev >= EVM_AION ? 500 : JITSchedule::logGas::value) + numTopics * (m_rev >= EVM_AION ? 500 : JITSchedule::logTopicGas::value);
	}

	case Instruction::CALL:
	case Instruction::CALLCODE:
	case Instruction::DELEGATECALL:
	case Instruction::STATICCALL:
		return m_rev >= EVM_AION ? 1000 : (m_rev >= EVM_TANGERINE_WHISTLE ? 700 : JITSchedule::callGas::value);

	case Instruction::CREATE:
		return m_rev >= EVM_AION ? 200000 : JITSchedule::createGas::value;

	case Instruction::SELFDESTRUCT:
		return  m_rev >= EVM_TANGERINE_WHISTLE ? 5000 : JITSchedule::stepGas0::value;

	default:
		// For invalid instruction just return 0.
		return 0;
	}
}

}
}
}
