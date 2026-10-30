"""Generate independent x86 CPU expectations and assembly input for Wasm tests.

Run on x86_64 with GNU gcc, as, objcopy and nm. No game files are read.
Expected results come from host instructions, never from our translator/v86.

Example:
  python3 crates/compiler/tests/fixtures/generate-native-flags.py \
    --output .tools/v86-native
  SIMS_FLAGS_EXPORT_INPUT="$PWD/.tools/v86-native/oracle-routines.json" \
    SIMS_FLAGS_EXPORT_OUTPUT="$PWD/.tools/v86-native/flags.wasm" \
    cargo test -p sims-compiler --lib export_flag_experiments -- --ignored
  node crates/compiler/tests/fixtures/check-native-flags.mjs \
    .tools/v86-native/flags.wasm .tools/v86-native
"""
import argparse, hashlib, pathlib, platform, struct, subprocess, json
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output',required=True,type=pathlib.Path)
args=parser.parse_args()
if platform.machine() not in ('x86_64','AMD64'):
    raise SystemExit('Native instruction oracle requires an x86_64 CPU')
out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
ops=[]
for width,suffix,a,b in [(8,'b','%al','%cl'),(16,'w','%ax','%cx'),(32,'l','%eax','%ecx')]:
 for name in ['add','sub','adc','sbb','and','or','xor','cmp','test']:
  ops.append((f'{name}{width}',f'{name}{suffix} {b}, {a}'))
 for name in ['inc','dec','neg']:
  ops.append((f'{name}{width}',f'{name}{suffix} {a}'))
ops += [('carry_chain','addl %ecx, %eax; movl %eax, %edx; xorl %edx, %eax; adcl %ecx, %eax'),('dead_chain','addl %ecx, %eax; movl %eax, %edx; xorl %edx, %eax; subl %ecx, %eax; cmpl %ecx, %eax'),('partial_chain','addb %cl, %al; incw %ax; sbbl %ecx, %eax'),('compare_setbe','cmpw %cx, %ax; setbe %al'),('compare_setl','cmpl %ecx, %eax; setl %al'),('compare_lahf','cmpl %ecx, %eax; lahf')]
source='.text\n.code32\n'+'\n'.join(f'.global {name}\n{name}:\n{asm}' for name,asm in ops)+'\n.global finish\nfinish:\n'
(out/'oracle.s').write_text(source)
subprocess.run(['as','--32',str(out/'oracle.s'),'-o',str(out/'oracle.o')],check=True)
subprocess.run(['objcopy','-O','binary','-j','.text',str(out/'oracle.o'),str(out/'oracle-code.bin')],check=True)
symbols={line.split()[2]:int(line.split()[0],16) for line in subprocess.check_output(['nm','-n',str(out/'oracle.o')],text=True).splitlines()}
blob=(out/'oracle-code.bin').read_bytes();routines=[]
for i,(name,_) in enumerate(ops):
 end=symbols[ops[i+1][0]] if i+1<len(ops) else symbols['finish']
 routines.append([name,list(blob[symbols[name]:end])])
(out/'oracle-routines.json').write_text(json.dumps(routines))
c='#include <stdio.h>\n#include <stdint.h>\n#include <stdlib.h>\n'
for name,asm in ops:
 escaped=asm.replace('%','%%').replace(';','; ')
 c+=f'''static void {name}(uint32_t a,uint32_t b,uint32_t initial,uint32_t out[2]){{
 uint64_t flags; uint64_t init=(initial&0x8d5)|2;
 __asm__ volatile ("pushq %[init]; popfq; {escaped}; pushfq; popq %[flags]" : "+a"(a), [flags]"=&r"(flags) : "c"(b), [init]"r"(init) : "edx","cc","memory");
 out[0]=a;out[1]=(uint32_t)flags;
}}\n'''
c+='static uint32_t seed=0x826a9b31;static uint32_t next(void){seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;return seed;}\n'
c+='int main(void){uint32_t edges[]={0,1,0xffffffff,0x7fffffff,0x80000000,0xff,0xffff,0x8000,0x80};\n'
for name,_ in ops:
 c+=f'''for(int i=0;i<4177;i++){{uint32_t a=next(),b=next(),f=next(),result[2];if(i<81){{a=edges[i%9];b=edges[i/9];}} {name}(a,b,f,result);uint32_t record[]={{a,b,f,result[0],result[1]}};if(fwrite(record,sizeof(record),1,stdout)!=1)return 1;}}\n'''
c+='return 0;}\n'
(out/'oracle.c').write_text(c)
subprocess.run(['gcc','-O2','-mno-red-zone','-Wall','-Wextra',str(out/'oracle.c'),'-o',str(out/'oracle')],check=True)
with (out/'oracle-results.bin').open('wb') as f:subprocess.run([str(out/'oracle')],stdout=f,check=True)
print(json.dumps({'routines':len(ops),'cases':len(ops)*4177,'mask':'CF/PF/ZF/SF/OF; AF excluded (not implemented for all widths); compare_lahf masks undefined/reserved AH bits'},indent=2))

# Keep a small, fixed edge/random corpus in the permanent regression suite.
data=(out/'oracle-results.bin').read_bytes()
selected=[0,1,8,9,10,20,40,44,50,60,70,80,81,82,83,84]
fixture_routines=[]
for i,(name,code) in enumerate(routines):
 fixture_routines.append({'name':name,'code':code,'cases':[list(struct.unpack_from('<5I',data,(i*4177+j)*20)) for j in selected]})
fixture={'provenance':{'method':'GNU inline assembly executed on an x86_64 host CPU; fixed xorshift32 seed 0x826a9b31, edge operands; no emulator used for expected results','nativeResultSha256':hashlib.sha256(data).hexdigest(),'flagMask':2245,'eaxMaskException':'compare_lahf excludes AH.AF (bit 12); AF is not implemented for all widths','upstreamInspiration':'copy/v86 native CPU differential testing; no upstream code copied'},'routines':fixture_routines}
lines=['{','  "provenance": '+json.dumps(fixture['provenance'])+',','  "routines": [']
for i,routine in enumerate(fixture_routines):
 lines += ['    {"name": '+json.dumps(routine['name'])+', "code": '+json.dumps(routine['code'])+', "cases": [']
 lines += ['      '+json.dumps(case)+(',' if j+1<len(routine['cases']) else '') for j,case in enumerate(routine['cases'])]
 lines += ['    ]}'+(',' if i+1<len(fixture_routines) else '')]
lines += ['  ]','}']
(out/'native-flags.json').write_text('\n'.join(lines)+'\n')
