    // Unsigned values keep all 32 bits; vector offsets are elements, not bytes.
    for (let count = 1; count <= 4; count++) {
        webGl2Method('uniform'+count+'ui',count+1,'-'+'u'.repeat(count),[[0,'WebGLUniformLocation',true]],function(location,...values) {
            const id = webGl2Handle(this,location,'WebGLUniformLocation',true);
            if (id > 0) webGl2Invoke(this,'uniform'+count+'ui',[id,...values.slice(0,count)]);
        });
        for (const kind of ['f','i','u']) {
            const name = 'uniform'+count+(kind === 'u' ? 'ui' : kind)+'v';
            webGl2Method(name,2,'--au',[[0,'WebGLUniformLocation',true],[1,webGl2NumericArgument(kind),false,true]],function(location,source,offset,length) {
                if (!webGlNumericValidate(this,source)) return;
                const id = webGl2Handle(this,location,'WebGLUniformLocation',true);
                if (id <= 0) return;
                const values = webGl2NumericSlice(this,source,offset,length);
                if (!values) return;
                if (kind === 'f') webGl2Invoke(this,name,[id],values);
                else webGl2Invoke(this,name,[id,...values]);
            });
        }
    }
    for (let columns = 2; columns <= 4; columns++) for (let rows = 2; rows <= 4; rows++) {
        const name = 'uniformMatrix'+columns+(rows===columns?'':'x'+rows)+'fv';
        webGl2Method(name,3,'-b-au',[[0,'WebGLUniformLocation',true],[2,webGl2NumericArgument('f'),false,true]],function(location,transpose,source,offset,length) {
            if (!webGlNumericValidate(this,source)) return;
            const id = webGl2Handle(this,location,'WebGLUniformLocation',true);
            if (id <= 0) return;
            const values = webGl2NumericSlice(this,source,offset,length);
            if (values) webGl2Invoke(this,name,[id,+transpose],values);
        });
    }
