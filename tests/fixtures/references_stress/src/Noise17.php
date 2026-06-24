<?php

namespace App;

class Noise17
{
    private function compute(): int
    {
        return 17;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
